export interface Env {
  RESEND_API_KEY: string;
  ADMIN_EMAIL: string;
  INBOX: KVNamespace;
  GITHUB_TOKEN?: string;
  // Map agent names to GitHub repos: "jenny=yiidtw/jenny-agent,infra=yiidtw/infra-agent"
  AGENT_REPOS?: string;
}

export default {
  async email(message: ForwardableEmailMessage, env: Env): Promise<void> {
    const to = message.to;
    const from = message.from;
    const subject = message.headers.get("subject") || "(no subject)";

    const localPart = to.split("@")[0];
    const parts = localPart.split(".");

    if (parts.length < 2) {
      await message.forward(env.ADMIN_EMAIL);
      return;
    }

    const agentName = parts[0];
    const username = parts.slice(1).join(".");

    const rawEmail = await new Response(message.raw).text();
    const body = extractTextBody(rawEmail);

    const msg = {
      id: crypto.randomUUID(),
      from,
      to,
      subject,
      timestamp: new Date().toISOString(),
      body,
    };

    // Append to per-user inbox (single key, no list() needed)
    const inboxKey = `inbox:${username}`;
    const existing = await env.INBOX.get(inboxKey);
    const messages: any[] = existing ? JSON.parse(existing) : [];
    messages.push(msg);
    // Keep max 50 messages, drop oldest
    while (messages.length > 50) messages.shift();
    await env.INBOX.put(inboxKey, JSON.stringify(messages), { expirationTtl: 86400 });

    console.log(`Email received: agent=${agentName} user=${username} from=${from} subject=${subject}`);

    // Create GitHub issue on agent's repo (if configured)
    await createGitHubIssue(env, agentName, from, subject, body);

    await sendReply(env.RESEND_API_KEY, from, agentName, username, subject, body);
    await message.forward(env.ADMIN_EMAIL);
  },

  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    // GET /inbox/:username — get all messages (single KV get, no list)
    if (request.method === "GET" && url.pathname.startsWith("/inbox/")) {
      const username = url.pathname.split("/")[2];
      if (!username) {
        return new Response(JSON.stringify({ error: "missing username" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        });
      }

      const raw = await env.INBOX.get(`inbox:${username}`);
      const messages = raw ? JSON.parse(raw) : [];
      return new Response(JSON.stringify({ messages }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    // DELETE /inbox/:username/:id — remove a message
    if (request.method === "DELETE" && url.pathname.match(/^\/inbox\/[^/]+\/[^/]+$/)) {
      const pathParts = url.pathname.split("/");
      const username = pathParts[2];
      const id = pathParts[3];

      const raw = await env.INBOX.get(`inbox:${username}`);
      if (raw) {
        const messages = JSON.parse(raw).filter((m: any) => m.id !== id);
        if (messages.length > 0) {
          await env.INBOX.put(`inbox:${username}`, JSON.stringify(messages), { expirationTtl: 86400 });
        } else {
          await env.INBOX.delete(`inbox:${username}`);
        }
      }
      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    // DELETE /inbox/:username — clear all
    if (request.method === "DELETE" && url.pathname.match(/^\/inbox\/[^/]+$/)) {
      const username = url.pathname.split("/")[2];
      await env.INBOX.delete(`inbox:${username}`);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("aide.sh email gateway", { status: 200 });
  },
};

function extractTextBody(rawEmail: string): string {
  const headerEnd = rawEmail.indexOf("\r\n\r\n");
  if (headerEnd === -1) return rawEmail;
  const body = rawEmail.substring(headerEnd + 4);
  const lines = body.split("\n");
  const textLines: string[] = [];
  let inText = true;
  for (const line of lines) {
    if (line.startsWith("--")) { inText = false; continue; }
    if (line.toLowerCase().startsWith("content-type: text/plain")) { inText = true; continue; }
    if (inText && !line.toLowerCase().startsWith("content-")) { textLines.push(line); }
  }
  return textLines.join("\n").trim().substring(0, 2000);
}

function parseAgentRepos(envVal: string | undefined): Record<string, string> {
  if (!envVal) return {};
  const map: Record<string, string> = {};
  for (const pair of envVal.split(",")) {
    const [agent, repo] = pair.trim().split("=");
    if (agent && repo) map[agent.trim()] = repo.trim();
  }
  return map;
}

async function createGitHubIssue(
  env: Env, agentName: string, from: string, subject: string, body: string,
): Promise<void> {
  if (!env.GITHUB_TOKEN || !env.AGENT_REPOS) return;

  const repos = parseAgentRepos(env.AGENT_REPOS);
  const repo = repos[agentName];
  if (!repo) {
    console.log(`No GitHub repo configured for agent: ${agentName}`);
    return;
  }

  try {
    const resp = await fetch(`https://api.github.com/repos/${repo}/issues`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${env.GITHUB_TOKEN}`,
        "Content-Type": "application/json",
        "User-Agent": "aide-email-gateway",
      },
      body: JSON.stringify({
        title: `[email] ${subject}`,
        body: `**From:** ${from}\n**Subject:** ${subject}\n\n---\n\n${body}\n\n---\n*Received via aide.sh email gateway*`,
        labels: ["email", "inbox"],
      }),
    });

    if (resp.ok) {
      const issue = await resp.json() as { number: number; html_url: string };
      console.log(`GitHub issue created: ${repo}#${issue.number}`);
    } else {
      console.error(`GitHub issue creation failed: ${resp.status} ${await resp.text()}`);
    }
  } catch (e) {
    console.error("Failed to create GitHub issue:", e);
  }
}

async function sendReply(
  apiKey: string, to: string, agentName: string, username: string,
  originalSubject: string, messageBody: string,
): Promise<void> {
  try {
    await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        from: `${agentName} <${agentName}.${username}@aide.sh>`,
        to: [to],
        subject: `Re: ${originalSubject}`,
        text: `Your message to ${agentName}.${username}@aide.sh has been received.\n\nAgent: ${agentName}\nMessage: ${messageBody.substring(0, 200)}\n\n---\naide.sh`,
      }),
    });
  } catch (e) {
    console.error("Failed to send reply:", e);
  }
}
