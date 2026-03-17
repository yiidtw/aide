export interface Env {
  RESEND_API_KEY: string;
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

    // For now, just forward to admin with metadata
    // TODO: When we have an API server, POST to it instead
    // For now, log the parsed info and forward
    console.log(
      `Email received: agent=${agentName} user=${username} from=${from} subject=${subject}`,
    );

    // Send auto-reply acknowledging receipt
    await sendReply(env.RESEND_API_KEY, from, agentName, username, subject, body);

    // Also forward original to admin for monitoring
    await message.forward("yiidtw@gmail.com");
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
