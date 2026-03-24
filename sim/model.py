"""
Mesa ABM v2: aide pseudo-life simulation.

Four rules (Go-like):
1. Do work → spend token. Get called → earn token. token < 0 → hibernate.
2. Qi (liberties): recent task flow toward you = qi. qi == 0 for K ticks → reclaimed.
3. No repeat: can't commit identical memory entry (ko rule).
4. Signal board: broadcast need → highest confidence bids → winner executes.

Life cycle: spawn → bid → exec → earn → grow
         or: can't bid → mutate (try new niche) or merge (donate memory, die)
"""

import math
import random
from dataclasses import dataclass, field
from typing import Optional

import mesa

# ── Task types ──
TASK_TYPES = ["web_search", "computation", "file_parse", "multi_step",
              "reasoning", "audio", "image", "hybrid"]

SKILLS = {
    "search":    {"web_search": 0.7, "multi_step": 0.3, "hybrid": 0.2},
    "fetch":     {"web_search": 0.5, "multi_step": 0.2, "hybrid": 0.1},
    "python":    {"computation": 0.8, "multi_step": 0.4, "reasoning": 0.5, "hybrid": 0.3},
    "readfile":  {"file_parse": 0.8, "image": 0.4, "audio": 0.4, "hybrid": 0.2},
    "youtube":   {"web_search": 0.3, "audio": 0.3},
    "reasoning": {"reasoning": 0.7, "computation": 0.3, "multi_step": 0.2},
}
ALL_SKILLS = list(SKILLS.keys())


@dataclass
class MemoryEntry:
    task_type: str
    strategy: frozenset
    reward: float
    tick: int
    source: str = "self"  # "self" or agent_id who donated


@dataclass
class Signal:
    tick: int
    from_id: int
    task_type: str
    context: str
    budget: int
    status: str = "open"       # open → claimed → done
    claimed_by: Optional[int] = None
    confidence: float = 0.0


@dataclass
class Task:
    task_id: int
    task_type: str
    difficulty: float
    required_skills: set


def generate_tasks(n, level=1, seed=42):
    rng = random.Random(seed)
    tasks = []
    for i in range(n):
        tt = rng.choice(TASK_TYPES)
        if level == 1:
            diff = rng.uniform(0.1, 0.4)
        elif level == 2:
            diff = rng.uniform(0.3, 0.7)
        else:
            diff = rng.uniform(0.5, 0.9)
        req = set()
        for skill, aff in SKILLS.items():
            if tt in aff and aff[tt] > 0.5:
                req.add(skill)
        if not req:
            req = {rng.choice(ALL_SKILLS)}
        if tt in ("multi_step", "hybrid"):
            extra = rng.sample(ALL_SKILLS, min(2 + level, len(ALL_SKILLS)))
            req.update(extra[:2 + level])
        tasks.append(Task(i, tt, diff, req))
    return tasks


def compute_reward(skills, task, memory):
    if not task.required_skills:
        coverage = 0.5
    else:
        coverage = len(skills & task.required_skills) / len(task.required_skills)
    base = 0.75 if coverage >= 1.0 else (0.4 if coverage >= 0.5 else 0.1)
    relevant = [m for m in memory if m.task_type == task.task_type and m.reward > 0.5]
    memory_boost = min(0.15 * len(relevant), 0.20)
    difficulty_penalty = task.difficulty * 0.3 * (1 - coverage)
    p = max(0.05, min(0.95, base + memory_boost - difficulty_penalty))
    return 1.0 if random.random() < p else 0.0


class AideAgent(mesa.Agent):

    def __init__(self, model, skills, token, parent_id=None, generation=0):
        super().__init__(model)
        self.skills = set(skills)
        self.token = token
        self.memory: list[MemoryEntry] = []
        self.memory_set: set[tuple] = set()   # for ko rule
        self.qi = 1                           # start alive
        self.qi_zero_ticks = 0
        self.hibernating = False
        self.tasks_attempted = 0
        self.tasks_correct = 0
        self.parent_id = parent_id
        self.generation = generation
        self.alive = True

    @property
    def accuracy(self):
        return self.tasks_correct / self.tasks_attempted if self.tasks_attempted > 0 else 0.0

    def confidence_for(self, task_type: str) -> float:
        relevant = [m for m in self.memory if m.task_type == task_type]
        if not relevant:
            return 0.0
        return sum(m.reward for m in relevant) / len(relevant)

    def step(self):
        if not self.alive:
            return

        model = self.model

        # Hibernate check
        if self.token < 0:
            self.hibernating = True
        if self.hibernating:
            # Can wake up if someone donates token (merge) or task pays upfront
            return

        # ── Qi check ──
        if self.qi <= 0:
            self.qi_zero_ticks += 1
            if self.qi_zero_ticks >= model.qi_death_threshold:
                self._die()
                return
        else:
            self.qi_zero_ticks = 0

        # Reset qi for this tick (will be incremented by being called/pulled)
        self.qi = 0

        # ── Phase 1: Look at signal board, try to claim work ──
        claimed_signal = self._try_claim_signal()

        if claimed_signal:
            self._execute_claimed(claimed_signal)
        else:
            # ── Phase 2: Get task from environment, post signal if needed ──
            task = model.get_task()
            if task is None:
                return

            # Can I do this myself?
            coverage = len(self.skills & task.required_skills) / len(task.required_skills) if task.required_skills else 0.5
            if coverage >= 0.5:
                self._execute_task(task)
            else:
                # Post signal — ask for help
                self._post_signal(task)
                # Also try myself (exploration)
                self._execute_task(task)

        # ── Phase 3: Life decisions ──
        if self.tasks_attempted >= 5:
            if self.accuracy >= 0.6 and self.token > model.spawn_cost * 2:
                self._maybe_spawn()
            elif self.accuracy < 0.3 and self.tasks_attempted >= 10:
                self._maybe_merge_or_mutate()

    def _try_claim_signal(self) -> Optional[Signal]:
        """Scan signal board for tasks I can do well."""
        best = None
        best_conf = 0.0
        for sig in self.model.signal_board:
            if sig.status != "open":
                continue
            conf = self.confidence_for(sig.task_type)
            # Also check skill coverage
            task_skills = set()
            for s, aff in SKILLS.items():
                if sig.task_type in aff and aff[sig.task_type] > 0.5:
                    task_skills.add(s)
            if task_skills and len(self.skills & task_skills) / len(task_skills) >= 0.5:
                conf = max(conf, 0.3)  # at least 0.3 if skills match
            if conf > best_conf and conf >= self.model.confidence_threshold:
                best_conf = conf
                best = sig
        if best:
            best.status = "claimed"
            best.claimed_by = self.unique_id
            best.confidence = best_conf
            self.qi += 1  # being called = qi
        return best

    def _execute_claimed(self, signal: Signal):
        """Execute a claimed signal's task."""
        # Synthesize a task from the signal
        task = Task(
            task_id=signal.tick * 1000 + signal.from_id,
            task_type=signal.task_type,
            difficulty=0.3,  # assume moderate
            required_skills=set(),
        )
        for s, aff in SKILLS.items():
            if signal.task_type in aff and aff[signal.task_type] > 0.5:
                task.required_skills.add(s)

        reward = compute_reward(self.skills, task, self.memory)
        cost = 1 + len(task.required_skills)
        self.token -= cost
        if self.token < 0:
            self.hibernating = True
        if reward > 0.5:
            self.token += signal.budget  # earn token from caller
            self.tasks_correct += 1
        self.tasks_attempted += 1

        # Commit memory (ko rule)
        self._commit_memory(task.task_type, frozenset(self.skills), reward)

        signal.status = "done"
        # Caller gets qi for having someone answer
        for a in self.model.agents:
            if isinstance(a, AideAgent) and a.unique_id == signal.from_id:
                a.qi += 1

    def _execute_task(self, task: Task):
        """Execute a task from the environment."""
        cost = 1 + len(task.required_skills)
        if self.token < cost:
            # Not enough token → hibernate immediately (TLA+ INV)
            if self.token < 0:
                self.hibernating = True
            return

        self.token -= cost
        # Check hibernate immediately after deduction (no gap between exec and hibernate)
        if self.token < 0:
            self.hibernating = True
        reward = compute_reward(self.skills, task, self.memory)
        if reward > 0.5:
            self.token += 2  # small reward from environment
            self.tasks_correct += 1
        self.tasks_attempted += 1
        self.qi += 1  # doing work = qi

        self._commit_memory(task.task_type, frozenset(self.skills), reward)

    def _commit_memory(self, task_type, strategy, reward):
        """Commit memory with ko rule check."""
        key = (task_type, strategy, reward > 0.5)
        if key in self.memory_set:
            return  # Ko rule: no duplicate
        self.memory_set.add(key)
        self.memory.append(MemoryEntry(task_type, strategy, reward, self.model.steps))

    def _post_signal(self, task: Task):
        """Post a help signal on the board."""
        budget = min(3, self.token)
        sig = Signal(
            tick=self.model.steps,
            from_id=self.unique_id,
            task_type=task.task_type,
            context=str(task.required_skills),
            budget=budget,
        )
        self.model.signal_board.append(sig)

    def _maybe_spawn(self):
        """Spawn child with filtered memory."""
        child_skills = set(self.skills)
        # Maybe add a learned skill
        learned_types = {m.task_type for m in self.memory if m.reward > 0.5}
        for s, aff in SKILLS.items():
            for tt in learned_types:
                if tt in aff and aff[tt] > 0.5 and s not in child_skills:
                    child_skills.add(s)
                    break

        child = AideAgent(
            self.model, child_skills, self.model.spawn_cost,
            parent_id=self.unique_id, generation=self.generation + 1,
        )
        # Transfer winning memory
        child.memory = [m for m in self.memory if m.reward > 0.5][-10:]
        for m in child.memory:
            child.memory_set.add((m.task_type, m.strategy, m.reward > 0.5))

        self.token -= self.model.spawn_cost
        self.qi += 1  # child = qi

    def _maybe_merge_or_mutate(self):
        """Low accuracy: mutate (explore new niche) or merge (donate memory)."""
        peers = [a for a in self.model.agents
                 if isinstance(a, AideAgent) and a.alive and a != self and a.accuracy > 0.5]

        if peers and random.random() < 0.7:
            # Merge: donate memory to strongest peer
            best = max(peers, key=lambda a: a.accuracy)
            for m in self.memory:
                key = (m.task_type, m.strategy, m.reward > 0.5)
                if key not in best.memory_set:
                    best.memory.append(MemoryEntry(
                        m.task_type, m.strategy, m.reward, m.tick, source=str(self.unique_id)
                    ))
                    best.memory_set.add(key)
            best.qi += 1  # merge = qi for receiver
            self._die()
        else:
            # Mutate: try a new random skill
            available = set(ALL_SKILLS) - self.skills
            if available:
                self.skills.add(random.choice(list(available)))

    def _die(self):
        """Agent dies. Memory already donated (or lost)."""
        self.alive = False
        self.token = 0
        self.model.death_count += 1


class GAIASwarmV2(mesa.Model):

    def __init__(
        self,
        n_agents=10,
        initial_token=100,
        n_tasks=53,
        task_level=1,
        spawn_cost=30,
        qi_death_threshold=5,
        confidence_threshold=0.3,
        seed=42,
    ):
        super().__init__(seed=seed)
        self.spawn_cost = spawn_cost
        self.qi_death_threshold = qi_death_threshold
        self.confidence_threshold = confidence_threshold
        self.signal_board: list[Signal] = []

        self.tasks = generate_tasks(n_tasks, level=task_level, seed=seed)
        self.task_idx = 0

        self.death_count = 0
        self.spawn_count = 0
        self.merge_count = 0
        self.history = []

        rng = random.Random(seed)
        for _ in range(n_agents):
            n_skills = rng.randint(2, 3)
            skills = set(rng.sample(ALL_SKILLS, n_skills))
            AideAgent(self, skills, initial_token)

    def get_task(self):
        task = self.tasks[self.task_idx % len(self.tasks)]
        self.task_idx += 1
        return task

    def step(self):
        alive = [a for a in self.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            self.running = False
            return

        # Clean old signals
        self.signal_board = [s for s in self.signal_board
                             if self.steps - s.tick < 5 and s.status == "open"]

        for agent in alive:
            agent.step()

        # Collect metrics
        all_a = [a for a in self.agents if isinstance(a, AideAgent)]
        alive = [a for a in all_a if a.alive]
        hibernating = [a for a in all_a if a.alive and a.hibernating]

        total_attempted = sum(a.tasks_attempted for a in all_a)
        total_correct = sum(a.tasks_correct for a in all_a)

        skill_dist = {}
        for a in alive:
            for s in a.skills:
                skill_dist[s] = skill_dist.get(s, 0) + 1

        self.history.append({
            "step": self.steps,
            "alive": len(alive),
            "hibernating": len(hibernating),
            "dead": sum(1 for a in all_a if not a.alive),
            "total_ever": len(all_a),
            "accuracy": total_correct / total_attempted if total_attempted > 0 else 0,
            "total_token": sum(a.token for a in alive),
            "avg_qi": sum(a.qi for a in alive) / len(alive) if alive else 0,
            "signals_open": sum(1 for s in self.signal_board if s.status == "open"),
            "max_gen": max((a.generation for a in all_a), default=0),
            "skill_dist": dict(skill_dist),
            "avg_memory": sum(len(a.memory) for a in alive) / len(alive) if alive else 0,
        })


def run_v2_experiments():
    """Run ablation on v2 model."""
    import json
    from pathlib import Path

    results_dir = Path("results/sim")
    results_dir.mkdir(parents=True, exist_ok=True)

    experiments = {
        "v2_baseline": {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 999},
        "v2_qi_death": {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 5},
        "v2_full": {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 5},
        "v2_small": {"n_agents": 5, "initial_token": 200, "qi_death_threshold": 5},
        "v2_large": {"n_agents": 30, "initial_token": 33, "qi_death_threshold": 5},
        "v2_rich": {"n_agents": 10, "initial_token": 500, "qi_death_threshold": 5},
        "v2_poor": {"n_agents": 10, "initial_token": 30, "qi_death_threshold": 5},
        "v2_L2": {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 5, "task_level": 2, "n_tasks": 86},
        "v2_L3": {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 5, "task_level": 3, "n_tasks": 26},
    }

    # Multi-seed for variance
    for seed in range(1, 6):
        experiments[f"v2_full_s{seed}"] = {"n_agents": 10, "initial_token": 100, "qi_death_threshold": 5, "seed": seed}

    all_results = {}
    for name, kwargs in experiments.items():
        n_steps = 300
        model = GAIASwarmV2(**kwargs)
        for _ in range(n_steps):
            alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
            if not alive:
                break
            model.step()

        all_a = [a for a in model.agents if isinstance(a, AideAgent)]
        alive = [a for a in all_a if a.alive]
        total_att = sum(a.tasks_attempted for a in all_a)
        total_cor = sum(a.tasks_correct for a in all_a)
        acc = total_cor / total_att if total_att > 0 else 0

        all_results[name] = {
            "accuracy": acc,
            "total_attempted": total_att,
            "alive": len(alive),
            "total_ever": len(all_a),
            "max_gen": max((a.generation for a in all_a), default=0),
            "deaths": model.death_count,
            "accuracy_curve": [h["accuracy"] for h in model.history[::max(1, len(model.history)//20)]],
            "alive_curve": [h["alive"] for h in model.history[::max(1, len(model.history)//20)]],
            "final_skills": {str(a.unique_id): list(a.skills) for a in alive},
        }

        print(f"{name:<25} acc={acc:.3f} alive={len(alive)}/{len(all_a)} "
              f"gen={max((a.generation for a in all_a), default=0)} deaths={model.death_count}")

    # Variance
    print("\nVariance (5 seeds):")
    accs = [all_results[f"v2_full_s{s}"]["accuracy"] for s in range(1, 6)]
    mean = sum(accs) / len(accs)
    std = (sum((a - mean)**2 for a in accs) / len(accs)) ** 0.5
    print(f"  v2_full: {mean:.3f} ± {std:.3f}")

    with open(results_dir / "ablation_v2.json", "w") as f:
        json.dump(all_results, f, indent=2)
    print(f"\nSaved to {results_dir / 'ablation_v2.json'}")

    return all_results


if __name__ == "__main__":
    run_v2_experiments()
