# Install Usage Meter

[Nederlands](INSTALL.nl.md)

Usage Meter shows how much of your Claude and ChatGPT limits you've used, as a
small gauge next to your clock. Setting it up takes about 5 minutes: you copy a
line, paste it into a window and press Enter. No technical knowledge needed.

> **Is this safe?** Usage Meter only reads your usage numbers (like "62% of
> this week"). It never reads your chats, never sees your password and sends
> nothing to your employer.

---

## Mac

### Step 1: Open Terminal

Press **⌘ Command + Space**, type **Terminal** and press **Enter**. A window
with text appears. That's where you'll paste the lines below.

To paste: **⌘ Command + V**, then press **Enter**.

### Step 2: Connect Claude (skip if you don't use Claude)

Usage Meter reads your Claude limits through *Claude Code*, Anthropic's free
helper app. If you already use Claude Code, go to step 3.

1. Paste this line and press Enter:

   ```
   curl -fsSL https://claude.ai/install.sh | bash
   ```

2. When it's done, **close Terminal and open it again** (step 1).
3. Type `claude` and press Enter. Your browser opens: **sign in with your
   Claude account** (the one you use at work).
4. Back in Terminal, when you see Claude Code's prompt, type `/exit` and press Enter.

### Step 3: Connect ChatGPT (skip if you don't use ChatGPT)

Usage Meter reads your ChatGPT limits through *Codex*, OpenAI's helper app.
If you already use Codex, go to step 4.

1. Paste this line and press Enter (this needs [Homebrew](https://brew.sh);
   if Terminal says `brew: command not found`, ask whoever sent you this guide):

   ```
   brew install --cask codex
   ```

2. Type `codex login` and press Enter. Your browser opens: choose
   **Sign in with ChatGPT** and use your work account.

### Step 4: Install Usage Meter

Paste this line and press Enter:

```
curl -fsSL https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-mac.sh | sh
```

A gauge with a percentage appears in the menu bar at the top right of your screen.

**Two questions your Mac may ask the first time:**

- *"security wants to use your confidential information stored in
  'Claude Code-credentials'"*: type your Mac password and click
  **Always Allow**. This lets Usage Meter use your Claude login.
- *"Usage Meter would like to send you notifications"*: click **Allow** to get
  a warning before you hit a limit.

---

## Windows

### Step 1: Open PowerShell

Click **Start**, type **PowerShell** and press **Enter**. A blue or black
window with text appears. That's where you'll paste the lines below.

To paste: **right-click** in the window (or **Ctrl + V**), then press **Enter**.

### Step 2: Connect Claude (skip if you don't use Claude)

1. Paste this line and press Enter:

   ```
   irm https://claude.ai/install.ps1 | iex
   ```

2. Look at the text that appears. Does it end with **Setup notes** saying
   *"is not in your PATH"*? Then also paste this line and press Enter.
   Nothing seems to happen; that's normal.

   ```
   [Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "User") + ";$env:USERPROFILE\.local\bin", "User")
   ```

3. **Close PowerShell and open it again** (step 1).
4. Type `claude` and press Enter. Your browser opens: **sign in with your
   Claude account**.

   Red text saying *"not recognized"*? Paste the long line from 2 anyway,
   press Enter and continue at 3.

5. Back in PowerShell, type `/exit` and press Enter.

### Step 3: Connect ChatGPT (skip if you don't use ChatGPT)

1. Codex needs Node.js. If you don't have it, download the **LTS** version
   from [nodejs.org](https://nodejs.org) and install it like any other program.
   Then close and reopen PowerShell.
2. Paste this line and press Enter:

   ```
   npm install -g @openai/codex
   ```

3. Type `codex login` and press Enter. Your browser opens: choose
   **Sign in with ChatGPT**.

### Step 4: Install Usage Meter

Paste this line and press Enter:

```
irm https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-windows.ps1 | iex
```

A gauge appears in the tray next to the clock (bottom right). Don't see it?
Click the small **^** arrow next to the clock; you can drag the gauge from
there onto the taskbar so it always shows.

---

## Using it

- **Click the gauge** to see your limits: the big rings are this week, the
  bars below are your 5-hour session and per-model limits.
- **Details** shows more: which models are blocked, credits, and what you used
  per model.
- **Settings** lets you choose the language (English or Nederlands), light or
  dark, and when you get a warning.
- It starts by itself when you log in and updates itself. You don't need to
  install it again.

## Something not right?

| You see | Do this |
|---|---|
| "Not connected" | Do step 2 or 3 again for that service. |
| "Login expired" | Open Claude Code once (`claude` in Terminal/PowerShell, then `/exit`). |
| Grey, old numbers | Click the refresh button at the top right of the popup, and check your internet. |
| No gauge at all | Run step 4 again. |

**Remove Usage Meter:** on a Mac, click the gauge › **Quit**, then drag
**Usage Meter** from your Applications folder to the Bin. On Windows, open
**Settings › Apps**, find **Usage Meter** and click **Uninstall**.

Still stuck? Ask whoever sent you this guide.
