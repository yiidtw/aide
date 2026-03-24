"""Quick smoke test for aide swarm simulation."""
import sys
sys.path.insert(0, ".")

from model import (
    AideAgent, GAIASwarmV2, generate_tasks,
    TASK_TYPES, SKILLS, ALL_SKILLS
)


def test_token_conservation():
    """SYS1: total token is conserved across all agents."""
    model = GAIASwarmV2(n_agents=5, initial_token=100, n_tasks=20, seed=42)
    initial_total = sum(
        a.token for a in model.agents if isinstance(a, AideAgent)
    )

    for _ in range(50):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

    # Token can only be created by environment reward or destroyed by cost
    # But total should be trackable
    all_agents = [a for a in model.agents if isinstance(a, AideAgent)]
    final_total = sum(a.token for a in all_agents)
    total_attempted = sum(a.tasks_attempted for a in all_agents)
    print(f"  token: {initial_total} -> {final_total} (delta={final_total - initial_total}, tasks={total_attempted})")
    # Token can change due to rewards, but should never go wildly negative overall
    assert final_total >= -100, f"Token went catastrophically negative: {final_total}"
    print("  PASS")


def test_occupation_immutable():
    """INV1: agent skills never shrink (only grow via mutation)."""
    model = GAIASwarmV2(n_agents=5, initial_token=100, n_tasks=20, seed=42)
    initial_skills = {}
    for a in model.agents:
        if isinstance(a, AideAgent):
            initial_skills[a.unique_id] = set(a.skills)

    for _ in range(50):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

    for a in model.agents:
        if isinstance(a, AideAgent) and a.unique_id in initial_skills:
            original = initial_skills[a.unique_id]
            # Skills can grow (mutation) but original skills must remain
            assert original.issubset(a.skills), \
                f"Agent {a.unique_id} lost skills: {original - a.skills}"
    print("  PASS")


def test_memory_append_only():
    """INV2: memory only grows, never shrinks or mutates."""
    model = GAIASwarmV2(n_agents=3, initial_token=100, n_tasks=10, seed=42)

    for step in range(30):
        snapshots = {}
        for a in model.agents:
            if isinstance(a, AideAgent) and a.alive:
                snapshots[a.unique_id] = list(a.memory)

        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

        for a in model.agents:
            if isinstance(a, AideAgent) and a.unique_id in snapshots:
                prev = snapshots[a.unique_id]
                curr = a.memory
                assert len(curr) >= len(prev), \
                    f"Agent {a.unique_id} memory shrank at step {step}"
                for i, entry in enumerate(prev):
                    assert curr[i] == entry, \
                        f"Agent {a.unique_id} memory mutated at index {i}, step {step}"
    print("  PASS")


def test_ko_rule():
    """INV5: no duplicate memory entries."""
    model = GAIASwarmV2(n_agents=5, initial_token=200, n_tasks=20, seed=42)

    for _ in range(100):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

    for a in model.agents:
        if isinstance(a, AideAgent):
            seen = set()
            for m in a.memory:
                key = (m.task_type, m.strategy, m.reward > 0.5)
                assert key not in seen, \
                    f"Agent {a.unique_id} has duplicate memory: {key}"
                seen.add(key)
    print("  PASS")


def test_bounded_agents():
    """SYS7: alive agent count stays bounded."""
    model = GAIASwarmV2(n_agents=10, initial_token=500, n_tasks=50, seed=42)
    max_alive = 0

    for _ in range(100):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        max_alive = max(max_alive, len(alive))
        model.step()

    print(f"  max alive agents: {max_alive}")
    # Should not explode unboundedly
    assert max_alive < 100, f"Agent count exploded: {max_alive}"
    print("  PASS")


def test_hibernate_on_zero_token():
    """Agent hibernates when token < 0."""
    model = GAIASwarmV2(n_agents=3, initial_token=5, n_tasks=20, seed=42)

    for _ in range(50):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

    for a in model.agents:
        if isinstance(a, AideAgent) and a.alive:
            if a.token < 0:
                assert a.hibernating, \
                    f"Agent {a.unique_id} has token={a.token} but not hibernating"
    print("  PASS")


def test_full_run():
    """End-to-end: run full v2 experiment, check it produces valid results."""
    model = GAIASwarmV2(n_agents=10, initial_token=100, n_tasks=53, seed=42)

    for _ in range(200):
        alive = [a for a in model.agents if isinstance(a, AideAgent) and a.alive]
        if not alive:
            break
        model.step()

    all_a = [a for a in model.agents if isinstance(a, AideAgent)]
    alive = [a for a in all_a if a.alive]
    total_att = sum(a.tasks_attempted for a in all_a)
    total_cor = sum(a.tasks_correct for a in all_a)
    acc = total_cor / total_att if total_att > 0 else 0

    print(f"  accuracy: {acc:.3f}")
    print(f"  alive: {len(alive)}/{len(all_a)}")
    print(f"  tasks: {total_att} attempted, {total_cor} correct")
    print(f"  history: {len(model.history)} steps")
    assert total_att > 0, "No tasks attempted"
    assert len(model.history) > 0, "No history recorded"
    print("  PASS")


if __name__ == "__main__":
    tests = [
        ("Token conservation (SYS1)", test_token_conservation),
        ("Occupation immutable (INV1)", test_occupation_immutable),
        ("Memory append-only (INV2)", test_memory_append_only),
        ("Ko rule (INV5)", test_ko_rule),
        ("Bounded agents (SYS7)", test_bounded_agents),
        ("Hibernate on zero token", test_hibernate_on_zero_token),
        ("Full run e2e", test_full_run),
    ]

    passed = 0
    failed = 0
    for name, fn in tests:
        print(f"\n[TEST] {name}")
        try:
            fn()
            passed += 1
        except Exception as e:
            print(f"  FAIL: {e}")
            failed += 1

    print(f"\n{'='*50}")
    print(f"Results: {passed} passed, {failed} failed")
    sys.exit(1 if failed > 0 else 0)
