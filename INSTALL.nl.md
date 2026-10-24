# Usage Meter installeren

[English](INSTALL.md)

Usage Meter toont hoeveel je van je Claude- en ChatGPT-limieten hebt gebruikt,
als een kleine meter naast je klok. Installeren duurt ongeveer 5 minuten: je
kopieert een regel, plakt die in een venster en drukt op Enter. Je hebt geen
technische kennis nodig.

> **Is dit veilig?** Usage Meter leest alleen je gebruikscijfers (zoals "62%
> van deze week"). Het leest nooit je gesprekken, ziet nooit je wachtwoord en
> stuurt niets naar je werkgever.

---

## Mac

### Stap 1: Terminal openen

Druk op **⌘ Command + spatie**, typ **Terminal** en druk op **Enter**. Er
verschijnt een venster met tekst. Daar plak je de regels hieronder.

Plakken: **⌘ Command + V**, daarna **Enter**.

### Stap 2: Claude verbinden (sla over als je Claude niet gebruikt)

Usage Meter leest je Claude-limieten via *Claude Code*, de gratis hulp-app van
Anthropic. Gebruik je Claude Code al? Ga dan naar stap 3.

1. Plak deze regel en druk op Enter:

   ```
   curl -fsSL https://claude.ai/install.sh | bash
   ```

2. Als het klaar is: **sluit Terminal en open het opnieuw** (stap 1).
3. Typ `claude` en druk op Enter. Je browser opent: **meld je aan met je
   Claude-account** (dat van je werk).
4. Terug in Terminal: typ `/exit` en druk op Enter.

### Stap 3: ChatGPT verbinden (sla over als je ChatGPT niet gebruikt)

Usage Meter leest je ChatGPT-limieten via *Codex*, de hulp-app van OpenAI.
Gebruik je Codex al? Ga dan naar stap 4.

1. Plak deze regel en druk op Enter (hiervoor heb je [Homebrew](https://brew.sh)
   nodig; zegt Terminal `brew: command not found`, vraag het dan aan wie je
   deze handleiding stuurde):

   ```
   brew install --cask codex
   ```

2. Typ `codex login` en druk op Enter. Je browser opent: kies
   **Sign in with ChatGPT** en gebruik je werkaccount.

### Stap 4: Usage Meter installeren

Plak deze regel en druk op Enter:

```
curl -fsSL https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-mac.sh | sh
```

Rechtsboven in de menubalk verschijnt een meter met een percentage.

**Twee vragen die je Mac de eerste keer kan stellen:**

- *"security wil je vertrouwelijke gegevens gebruiken die zijn bewaard in
  'Claude Code-credentials'"*: typ je Mac-wachtwoord en klik op
  **Sta altijd toe**. Zo kan Usage Meter je Claude-login gebruiken.
- *"Usage Meter wil je berichten sturen"*: klik op **Sta toe** om een
  waarschuwing te krijgen voordat je een limiet bereikt.

---

## Windows

### Stap 1: PowerShell openen

Klik op **Start**, typ **PowerShell** en druk op **Enter**. Er verschijnt een
blauw of zwart venster met tekst. Daar plak je de regels hieronder.

Plakken: **rechtsklik** in het venster (of **Ctrl + V**), daarna **Enter**.

### Stap 2: Claude verbinden (sla over als je Claude niet gebruikt)

1. Plak deze regel en druk op Enter:

   ```
   irm https://claude.ai/install.ps1 | iex
   ```

2. Kijk naar de tekst die verschijnt. Staat er onderaan **Setup notes** met
   *"is not in your PATH"*? Plak dan ook deze regel en druk op Enter. Je ziet
   niets gebeuren, dat is normaal.

   ```
   [Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "User") + ";$env:USERPROFILE\.local\bin", "User")
   ```

3. **Sluit PowerShell en open het opnieuw** (stap 1).
4. Typ `claude` en druk op Enter. Je browser opent: **meld je aan met je
   Claude-account**.

   Krijg je rode tekst met *"not recognized"* of *"niet herkend"*? Plak dan
   alsnog de lange regel uit 2, druk op Enter en ga verder bij 3.

5. Terug in PowerShell: typ `/exit` en druk op Enter.

### Stap 3: ChatGPT verbinden (sla over als je ChatGPT niet gebruikt)

1. Codex heeft Node.js nodig. Heb je dat niet? Download de **LTS**-versie van
   [nodejs.org](https://nodejs.org) en installeer het zoals elk ander
   programma. Sluit en open PowerShell daarna opnieuw.
2. Plak deze regel en druk op Enter:

   ```
   npm install -g @openai/codex
   ```

3. Typ `codex login` en druk op Enter. Je browser opent: kies
   **Sign in with ChatGPT**.

### Stap 4: Usage Meter installeren

Plak deze regel en druk op Enter:

```
irm https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-windows.ps1 | iex
```

Er verschijnt een meter in het systeemvak naast de klok (rechtsonder). Zie je
hem niet? Klik op het pijltje **^** naast de klok; van daaruit kun je de meter
naar de taakbalk slepen zodat hij altijd zichtbaar is.

---

## Gebruik

- **Klik op de meter** om je limieten te zien: de grote ringen zijn deze week,
  de balken eronder je sessie van 5 uur en de limieten per model.
- **Details** toont meer: welke modellen geblokkeerd zijn, tegoed, en je
  gebruik per model.
- Bij **Instellingen** kies je de taal (Nederlands of English), licht of
  donker, en wanneer je een waarschuwing krijgt.
- Het start vanzelf als je inlogt en werkt zichzelf bij. Je hoeft het niet
  opnieuw te installeren.

## Klopt er iets niet?

| Je ziet | Doe dit |
|---|---|
| "Niet verbonden" | Doe stap 2 of 3 opnieuw voor die dienst. |
| "Login verlopen" | Open Claude Code één keer (`claude` in Terminal/PowerShell, daarna `/exit`). |
| Grijze, oude cijfers | Klik op de vernieuwknop rechtsboven in het venster en controleer je internet. |
| Helemaal geen meter | Doe stap 4 opnieuw. |

**Usage Meter verwijderen:** op een Mac klik je op de meter › **Stoppen** en
sleep je **Usage Meter** uit de map Apps naar de prullenmand. Op Windows open je
**Instellingen › Apps**, zoek je **Usage Meter** en klik je op **Verwijderen**.

Kom je er niet uit? Vraag het aan wie je deze handleiding stuurde.
