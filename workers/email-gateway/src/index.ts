export interface Env {
  RESEND_API_KEY: string;
  INBOX: KVNamespace;
}

export default {
  async email(message: ForwardableEmailMessage, env: Env): Promise<void> {
    const to = message.to; // e.g. "school.ydwu@aide.sh"
    const from = message.from; // sender's email
    const subject = message.headers.get("subject") || "(no subject)";

    // Parse recipient: agent.username@aide.sh
    const localPart = to.split("@")[0]; // "school.ydwu"
    const parts = localPart.split(".");

    if (parts.length < 2) {
      // Not an agent address, forward to admin
      await message.forward("yiidtw@gmail.com");
      return;
    }

    const agentName = parts[0]; // "school"
    const username = parts.slice(1).join("."); // "ydwu"

    // Read email body
    const rawEmail = await new Response(message.raw).text();
    // Extract plain text body (simple extraction)
    const body = extractTextBody(rawEmail);

    // Store message in KV for daemon polling
    const msg = {
      id: crypto.randomUUID(),
      from,
      to,
      subject,
      timestamp: new Date().toISOString(),
      body,
    };

    await env.INBOX.put(`msg:${msg.id}`, JSON.stringify(msg), {
      expirationTtl: 86400,
    }); // 24h TTL

    console.log(
      `Email received: agent=${agentName} user=${username} from=${from} subject=${subject}`,
    );

    // Send auto-reply acknowledging receipt
    await sendReply(env.RESEND_API_KEY, from, agentName, username, subject, body);

    // Also forward original to admin for monitoring
    await message.forward("yiidtw@gmail.com");
  },

  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    // GET /inbox/:username — list messages for this user
    if (request.method === "GET" && url.pathname.startsWith("/inbox/")) {
      const username = url.pathname.split("/")[2];
      if (!username) {
        return new Response(JSON.stringify({ error: "missing username" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        });
      }

      const list = await env.INBOX.list({ prefix: "msg:" });
      const messages = [];
      for (const key of list.keys) {
        const raw = await env.INBOX.get(key.name);
        if (raw) {
          const parsed = JSON.parse(raw);
          // Filter by username (to field contains username)
          if (parsed.to.includes(username)) {
            messages.push(parsed);
          }
        }
      }
      return new Response(JSON.stringify({ messages }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    // DELETE /inbox/:username/:id — ack a message
    if (
      request.method === "DELETE" &&
      url.pathname.match(/^\/inbox\/[^/]+\/[^/]+$/)
    ) {
      const parts = url.pathname.split("/");
      const id = parts[3];
      await env.INBOX.delete(`msg:${id}`);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("aide.sh email gateway", { status: 200 });
  },
};

function extractTextBody(rawEmail: string): string {
  // Simple: find the first blank line (end of headers), take the rest
  const headerEnd = rawEmail.indexOf("\r\n\r\n");
  if (headerEnd === -1) return rawEmail;
  const body = rawEmail.substring(headerEnd + 4);
  // Strip any MIME boundaries (very basic)
  const lines = body.split("\n");
  const textLines: string[] = [];
  let inText = true;
  for (const line of lines) {
    if (line.startsWith("--")) {
      inText = false;
      continue;
    }
    if (line.toLowerCase().startsWith("content-type: text/plain")) {
      inText = true;
      continue;
    }
    if (inText && !line.toLowerCase().startsWith("content-")) {
      textLines.push(line);
    }
  }
  return textLines.join("\n").trim().substring(0, 2000); // truncate
}

async function sendReply(
  apiKey: string,
  to: string,
  agentName: string,
  username: string,
  originalSubject: string,
  messageBody: string,
): Promise<void> {
  const replyBody = `Your message to ${agentName}.${username}@aide.sh has been received.

Agent: ${agentName}
User: ${username}
Message: ${messageBody.substring(0, 200)}

---
This is an automated reply from aide.sh.
The agent will process your message and respond when ready.

To deploy your own agents: https://aide.sh`;

  try {
    await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        from: `${agentName} <${agentName}.${username}@aide.sh>`,
        to: [to],
        subject: `Re: ${originalSubject}`,
        text: replyBody,
      }),
    });
  } catch (e) {
    console.error("Failed to send reply:", e);
  }
}
