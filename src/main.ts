import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t, phrase, setLang, resolveLang, locale, lang, type Key } from "./i18n";


interface Limit { label: string; window: string | null; model: string | null; percent: number; resets_at: number | null; critical: boolean }
interface Detail { label: string; text: string; tone: string; at?: number }
interface ModelUsage { model: string; h5: [number, number]; week: [number, number]; blocked_at: number | null }
interface UsageTable { source: string; unit: string; token_unit: string; models: ModelUsage[] }
interface Account {
  id: string; kind: "claude" | "codex"; name: string; plan: string;
  limits: Limit[]; details: Detail[]; note: string | null; block_reason: string | null;
  usage: UsageTable | null; error: string | null;
  connected: boolean; fetched_at: number | null;
}
interface Snapshot { accounts: Account[]; updated_at: number }
interface Settings {
  theme: "system" | "light" | "dark"; language: "system" | "en" | "nl";
  refresh_minutes: number; warn_at: number; notify_full: boolean; notify_reset: boolean;
}

type View = "overview" | "details" | "settings";

const STALE_AFTER = 30 * 60;
const WEEK = 7 * 86400;
const USAGE_PAGES: Record<string, string> = {
  claude: "https://claude.ai/settings/usage",
  codex: "https://chatgpt.com/codex/settings/usage",
};
const IS_WINDOWS = navigator.userAgent.includes("Windows");
// What a colleague pastes to get Claude Code or Codex signed in (same as INSTALL.md).
const SETUP: Record<Account["kind"], { install: string; signIn: string }> = {
  claude: {
    install: IS_WINDOWS ? "irm https://claude.ai/install.ps1 | iex" : "curl -fsSL https://claude.ai/install.sh | bash",
    signIn: "claude",
  },
  codex: {
    install: IS_WINDOWS ? "npm install -g @openai/codex" : "brew install --cask codex",
    signIn: "codex login",
  },
};
const GUIDE = "https://github.com/nealwebdev/usage-meter/blob/main/";
// Language names are written in their own language.
const LANGS: [Settings["language"], string][] = [["system", ""], ["en", "English"], ["nl", "Nederlands"]];

const state = {
  view: "overview" as View,
  snap: { accounts: [], updated_at: 0 } as Snapshot,
  settings: { theme: "system", language: "system", refresh_minutes: 5, warn_at: 80, notify_full: true, notify_reset: true } as Settings,
  refreshing: false,
  update: null as Update | null,
  updateReady: false,
  updateMsg: "",
  version: "",
  autostart: false,
  skipWelcome: false,
  welcomeMsg: "",
  copied: "",
};

const app = document.getElementById("app")!;


const esc = (s: unknown) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
const now = () => Date.now() / 1000;
const pct = (n: number) => `${Math.round(n)}%`;
const isStale = (a: Account) => !a.fetched_at || now() - a.fetched_at > STALE_AFTER;
const connectHint = (a: Account) => t(a.kind === "claude" ? "connectClaude" : "connectCodex");

/** "15:00" today, "Mon 20:00" / "ma 20:00" this week, "31 Oct" / "31 okt" further out. */
function fmtReset(ts: number | null): string {
  if (!ts) return "";
  const when = new Date(Math.round(ts / 60) * 60 * 1000);
  const today = new Date();
  const hm = when.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
  if (when.toDateString() === today.toDateString()) return hm;
  if (Math.abs(when.getTime() - today.getTime()) < 6 * 86400 * 1000)
    return `${when.toLocaleDateString(locale(), { weekday: "short" }).replace(".", "")} ${hm}`;
  return when.toLocaleDateString(locale(), { day: "numeric", month: "short" }).replace(".", "");
}

function fmtIn(ts: number | null): string {
  if (!ts) return "";
  const mins = Math.max(0, Math.round((ts - now()) / 60));
  if (mins < 60) return t("inMin", { m: mins });
  if (mins < 24 * 60) return t("inHours", { h: Math.floor(mins / 60), m: String(mins % 60).padStart(2, "0") });
  return fmtReset(ts);
}

function fmtAge(ts: number, updated = true): string {
  if (!ts) return t("loading");
  const mins = Math.floor((now() - ts) / 60);
  if (mins < 1) return updated ? t("updatedNow") : t("justNow");
  if (mins < 90) return t(updated ? "updatedMin" : "agoMin", { n: mins });
  return t(updated ? "updatedHours" : "agoHours", { n: Math.floor(mins / 60) });
}

function human(n: number): string {
  for (const [unit, size] of [["B", 1e9], ["M", 1e6], ["k", 1e3]] as const) {
    if (n >= size) {
      const v = n / size;
      return (v >= 100 ? v.toFixed(0) : v.toFixed(1).replace(/\.0$/, "")) + unit;
    }
  }
  return String(n);
}

/** Full label in the current language: "Weekly · Fable" / "Wekelijks · Fable". */
function limitLabel(l: Limit): string {
  const base = l.window === "5h" ? t("session5h") : l.window === "week" ? t("weekly") : null;
  if (!base) return phrase(l.label);
  return l.model ? `${base} · ${phrase(l.model)}` : base;
}

/** Short label for the overview: "Claude · 5h", "Claude · Fable week". */
function shortLabel(a: Account, l: Limit): string {
  let short: string;
  if (l.model) short = l.window === "5h" ? t("model5h", { model: phrase(l.model) }) : l.window === "week" ? t("modelWeek", { model: phrase(l.model) }) : phrase(l.model);
  else short = l.window === "5h" ? t("h5") : limitLabel(l);
  return `${a.name} · ${short}`;
}

/** The headline limit for an account: plan-wide weekly, else the first one. */
const headline = (a: Account) => a.limits.find((l) => l.window === "week" && !l.model) ?? a.limits[0];

function tone(l: Limit): "" | "warn" | "bad" {
  if (l.critical || l.percent >= 100) return "bad";
  return l.percent >= state.settings.warn_at && state.settings.warn_at > 0 ? "warn" : "";
}

/** What can I use right now? */
function status(a: Account): { text: string; tone: "ok" | "warn" | "bad" } | null {
  if (!a.limits.length) return null;
  const full = a.limits.filter((l) => !l.model && l.percent >= 100);
  if (full.length) {
    const until = Math.max(...full.map((l) => l.resets_at ?? 0));
    let text = until ? t("allBlockedUntil", { t: fmtReset(until) }) : t("allBlocked");
    if (a.block_reason) text += ` · ${phrase(a.block_reason)}`;
    return { text, tone: "bad" };
  }
  const capped = a.limits.filter((l) => l.model && l.percent >= 100);
  if (capped.length) {
    const names = capped.map((l) => phrase(l.model!)).join(", ");
    return { text: t("someBlocked", { names, t: fmtReset(capped[0].resets_at) }), tone: "warn" };
  }
  return { text: t("allAvailable"), tone: "ok" };
}

/** "At this pace … runs out Wed 16:00", when a weekly limit is on course to run out before it resets. */
function forecast(a: Account): string | null {
  const l = headline(a);
  if (!l || l.window !== "week" || !l.resets_at || l.percent >= 100 || l.percent < 20 || isStale(a)) return null;
  const elapsed = now() - (l.resets_at - WEEK);
  if (elapsed < 6 * 3600) return null;
  const hitAt = now() + (100 - l.percent) / (l.percent / elapsed);
  return hitAt < l.resets_at - 1800 ? t("forecast", { name: esc(a.name), t: fmtReset(hitAt) }) : null;
}

function detailText(d: Detail): string {
  let text = phrase(d.text);
  if (d.at) text = text.replace(phrase("unavailable"), t("unavailableUntil", { t: fmtReset(d.at) }));
  return text;
}

const ICON = {
  refresh: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/></svg>`,
  back: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M15 6l-6 6 6 6"/></svg>`,
  details: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/></svg>`,
  settings: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/></svg>`,
  quit: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3v9M6.3 6.3a8 8 0 1 0 11.4 0"/></svg>`,
  warn: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3l9 16H3z"/><path d="M12 10v4M12 17h.01"/></svg>`,
  block: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M8 8l8 8"/></svg>`,
  ok: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M5 12l5 5 9-10"/></svg>`,
  update: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 4v11M7 10l5 5 5-5M5 20h14"/></svg>`,
};


function ring(a: Account): string {
  const l = headline(a);
  const r = 40, c = 2 * Math.PI * r;
  const p = l ? Math.min(l.percent, 100) : 0;
  const val = l ? `<span>${Math.round(l.percent)}<small>%</small></span>` : `<span class="faint">–</span>`;
  return `<div class="ring ${l ? "" : "off"}" role="img" aria-label="${esc(a.name)} ${l ? pct(l.percent) : "–"}">
    <svg viewBox="0 0 100 100" aria-hidden="true"><circle class="track" cx="50" cy="50" r="${r}"/>
    ${l ? `<circle class="arc ${tone(l) === "bad" ? "bad" : ""}" cx="50" cy="50" r="${r}" stroke-dasharray="${(c * p) / 100} ${c}"/>` : ""}</svg>
    <div class="val mono">${val}</div></div>`;
}

function tile(a: Account): string {
  const l = headline(a);
  let meta: string;
  if (!a.connected) meta = `<div class="hint">${connectHint(a)}</div>`;
  else if (!l) meta = `<div class="hint">${esc(a.error ? phrase(a.error) : t("loading"))}</div>`;
  else {
    const label = l.window === "week" ? t(a.kind === "codex" ? "codexWeek" : "week") : esc(limitLabel(l));
    meta = `<div class="meta">${label}${l.resets_at ? ` · ${t("resets", { t: fmtReset(l.resets_at) })}` : ""}</div>`;
  }
  return `<div class="tile ${a.kind}" style="${isStale(a) && l ? "opacity:.6" : ""}">
    ${ring(a)}
    <div class="name"><span class="dot"></span>${esc(a.name)}${a.plan ? `<span class="faint" style="font-weight:500;font-size:11px">${esc(a.plan)}</span>` : ""}</div>
    ${meta}</div>`;
}

function limitRow(a: Account, l: Limit, short: boolean): string {
  const label = short ? shortLabel(a, l) : limitLabel(l);
  const reset =
    l.percent >= 100 && l.model ? t("blockedUntil", { t: fmtReset(l.resets_at) })
    : l.resets_at ? t("resets", { t: l.window === "5h" ? fmtIn(l.resets_at) : fmtReset(l.resets_at) }) : "";
  return `<div class="row ${a.kind} ${tone(l)}">
    <span class="label" title="${esc(label)}">${esc(label)}</span>
    <div class="bar"><i style="width:${Math.min(l.percent, 100)}%"></i></div>
    <span class="pct mono">${l.percent >= 100 ? t("max") : pct(l.percent)}</span>
    ${reset ? `<span class="reset">${reset}</span>` : ""}</div>`;
}

const refreshButton = () =>
  `<button class="icon-btn ${state.refreshing ? "spin" : ""}" data-act="refresh" aria-label="${t("refreshNow")}" title="${t("refreshNow")}">${ICON.refresh}</button>`;

function header(): string {
  const anyError = state.snap.accounts.some((a) => a.connected && a.error && isStale(a));
  return `<div class="head">
    <div><div class="title">USAGE METER</div>
      <div class="sub"><span class="live ${anyError || !state.snap.updated_at ? "off" : ""}"></span>${fmtAge(state.snap.updated_at)}</div></div>
    ${refreshButton()}
  </div>`;
}

function updateBanner(): string {
  if (!state.updateReady || !state.update) return "";
  return `<div class="notice info">${ICON.update}<span class="grow">${t("updateReady", { v: esc(state.update.version) })}</span>
    <button data-act="restart">${t("restart")}</button></div>`;
}


function overview(): string {
  const accounts = state.snap.accounts;
  const rows = accounts.flatMap((a) => a.limits.filter((l) => l !== headline(a)).map((l) => limitRow(a, l, true))).join("");
  const notes = accounts.map(forecast).filter(Boolean);
  return `${header()}
  <div class="body">
    ${updateBanner()}
    <div class="tiles">${accounts.map(tile).join("")}</div>
    ${rows ? `<div class="caps" style="margin-top:4px">${esc(t("sessionsModelLimits"))}</div><div class="rows">${rows}</div>` : ""}
    ${notes.map((n) => `<div class="notice warn">${ICON.warn}<span><b>${t("headsUp")}</b><br>${n}</span></div>`).join("")}
  </div>
  <div class="foot">
    <button data-view="details">${ICON.details}${t("details")}</button>
    <button data-view="settings">${ICON.settings}${t("settings")}</button>
    <button data-act="quit">${ICON.quit}${t("quit")}</button>
  </div>`;
}

/** Nothing signed in yet: explain the one thing to do first. */
const showWelcome = () =>
  !state.skipWelcome && state.snap.accounts.length > 0 && state.snap.accounts.every((a) => !a.connected);

function command(text: string): string {
  const copied = state.copied === text;
  return `<div class="cmd"><code>${esc(text)}</code>
    <button class="copy" data-copy="${esc(text)}">${t(copied ? "copied" : "copy")}</button></div>`;
}

function welcome(): string {
  const cards = state.snap.accounts
    .map((a) => `<div class="setup ${a.kind}">
      <div class="name"><span class="dot"></span>${esc(a.name)}<span class="faint" style="font-weight:500;font-size:11px">${t("notConnected")}</span></div>
      ${command(SETUP[a.kind].install)}
      ${command(SETUP[a.kind].signIn)}
      <span class="muted" style="font-size:11px">${t(a.kind === "claude" ? "stepSignInClaude" : "stepSignInCodex")}</span>
    </div>`)
    .join("");
  return `<div class="head"><div><div class="title">USAGE METER</div>
    <div class="sub"><span class="live off"></span>${t("notConnected")}</div></div></div>
  <div class="body welcome">
    <div><h1>${t("welcomeTitle")}</h1><p class="muted">${t("welcomeIntro")}</p></div>
    <span class="muted" style="font-size:12px">${t("welcomeWhere", { app: IS_WINDOWS ? "PowerShell" : "Terminal" })}</span>
    ${cards}
    ${state.welcomeMsg ? `<div class="notice warn">${ICON.warn}<span>${esc(state.welcomeMsg)}</span></div>` : ""}
    <button class="btn-primary ${state.refreshing ? "busy" : ""}" data-act="check-login">${t("checkAgain")}</button>
    <div class="links"><button class="link" data-act="guide">${t("fullGuide")}</button>
      <button class="link" data-act="skip-welcome">${t("skipForNow")}</button></div>
    <span class="faint" style="font-size:11px">${t("welcomePrivacy")} ${t("welcomeReopen", { bar: t(IS_WINDOWS ? "tray" : "menuBar") })}</span>
  </div>
  <div class="foot">
    <button data-view="settings">${ICON.settings}${t("settings")}</button>
    <button data-act="quit">${ICON.quit}${t("quit")}</button>
  </div>`;
}

/** The clipboard API can be missing in a webview; fall back to a hidden text field. */
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const field = document.createElement("textarea");
    field.value = text;
    document.body.append(field);
    field.select();
    document.execCommand("copy");
    field.remove();
  }
}

function usageTable(u: UsageTable): string {
  const unit = (count: number) => t(`unit_${count === 1 ? u.unit.replace(/s$/, "") : u.unit}` as Key);
  const cell = ([count, tokens]: [number, number]) =>
    count || tokens ? `${human(count)} ${unit(count)} · ${human(tokens)} ${t(`unit_${u.token_unit}` as Key)}` : `<span class="faint">–</span>`;
  return `<div class="table-caption"><span class="caps">${t("byModel")}</span><span class="faint" style="font-size:11px">${t("onThisComputer", { source: esc(u.source) })}</span></div>
  <div class="table">
    <div class="tr"><span>${t("colModel")}</span><span>${t("colLast5h")}</span><span>${t("colThisWeek")}</span></div>
    ${u.models
      .map((m) => {
        const hit = m.blocked_at && now() - m.blocked_at < 5 * 3600;
        return `<div class="tr" ${hit ? `style="color:var(--red)" title="${t("limitHit", { t: fmtReset(m.blocked_at) })}"` : ""}><span>${esc(m.model)}</span><span class="mono">${cell(m.h5)}</span><span class="mono">${cell(m.week)}</span></div>`;
      })
      .join("")}
  </div>`;
}

function details(): string {
  const sections = state.snap.accounts.map((a) => {
    const name = `<span class="name"><span class="dot ${a.kind}"></span>${esc(a.name)}</span>`;
    if (!a.connected)
      return `<div class="section"><div class="section-head">${name}</div>
        <div class="notice info"><span>${connectHint(a)}. ${t("reusesLogin")}</span></div></div>`;
    const st = isStale(a) ? null : status(a);
    const stIcon = st?.tone === "ok" ? ICON.ok : st?.tone === "bad" ? ICON.block : ICON.warn;
    const error = a.error
      ? `<div class="notice ${isStale(a) ? "warn" : "info"}">${ICON.warn}<span>${esc(phrase(a.error))}${a.fetched_at ? ` · ${t("lastGood", { age: fmtAge(a.fetched_at, false) })}` : ""}</span></div>`
      : a.note ? `<div class="faint" style="font-size:11px">${esc(phrase(a.note))}</div>` : "";
    return `<div class="section">
      <div class="section-head">${name}${a.plan ? `<span class="pill">${esc(a.plan)}</span>` : ""}</div>
      <div class="rows">${a.limits.map((l) => limitRow(a, l, false)).join("")}</div>
      ${st ? `<div class="notice ${st.tone}">${stIcon}<span>${esc(st.text)}</span></div>` : ""}
      ${a.details.length ? `<div class="kv">${a.details.map((d) => `<span class="k">${esc(phrase(d.label))}</span><span class="tone-${esc(d.tone)}">${esc(detailText(d))}</span>`).join("")}</div>` : ""}
      ${a.usage?.models.length ? usageTable(a.usage) : ""}
      ${error}
      <div><button class="link" data-open="${a.kind}">${t(a.kind === "claude" ? "openUsageClaude" : "openUsageCodex")}</button></div>
    </div>`;
  });
  return `<div class="head"><button class="back" data-view="overview">${ICON.back}${t("details")}</button>${refreshButton()}</div>
  <div class="body" style="gap:0">${sections.join("")}</div>`;
}

function segmented(name: string, options: [string, string][], selected: string): string {
  return `<div class="seg" role="radiogroup" aria-label="${esc(name)}">${options
    .map(([id, label]) => `<button role="radio" aria-checked="${selected === id}" data-${name}="${id}">${esc(label)}</button>`)
    .join("")}</div>`;
}

function settingsView(): string {
  const s = state.settings;
  const themeHint = t(s.theme === "system" ? "themeHintSystem" : s.theme === "light" ? "themeHintLight" : "themeHintDark");
  const langs = LANGS.map(([id, label]) => [id, id === "system" ? t("languageSystem") : label] as [string, string]);
  const accounts = state.snap.accounts
    .map((a) => `<div class="item"><span style="display:flex;align-items:center;gap:8px"><span class="dot ${a.kind}"></span>${esc(a.name)}${a.plan ? ` · ${esc(a.plan)}` : ""}</span>
      <span class="${a.connected ? "tone-ok" : "faint"}" style="font-size:12px">${t(a.connected ? "connected" : "notConnected")}</span></div>
      ${a.connected ? "" : `<div class="item faint" style="font-size:12px;padding-top:0">${connectHint(a)}</div>`}`)
    .join("");
  return `<div class="head"><button class="back" data-view="overview">${ICON.back}${t("settings")}</button></div>
  <div class="body">
    <div class="group"><span class="caps">${t("appearance")}</span>
      <div class="card"><div class="item stack">
        ${segmented("theme", [["system", t("themeSystem")], ["light", t("themeLight")], ["dark", t("themeDark")]], s.theme)}
        <span class="muted" style="font-size:12px">${themeHint}</span></div></div></div>

    <div class="group"><span class="caps">${t("language")}</span>
      <div class="card"><div class="item stack">
        ${segmented("lang", langs, s.language)}
        ${s.language === "system" ? `<span class="muted" style="font-size:12px">${t("languageHintSystem")}</span>` : ""}</div></div></div>

    <div class="group"><span class="caps">${t("general")}</span><div class="card">
      <label class="item"><span>${t("openAtLogin")}</span><input type="checkbox" data-set="autostart" ${state.autostart ? "checked" : ""}></label>
      <div class="item"><label for="refresh">${t("checkEvery")}</label>
        <select id="refresh" data-set="refresh_minutes">${[5, 10, 15, 30].map((m) => `<option value="${m}" ${s.refresh_minutes === m ? "selected" : ""}>${t("minutes", { n: m })}</option>`).join("")}</select></div>
    </div></div>

    <div class="group"><span class="caps">${t("alerts")}</span><div class="card">
      <div class="item stack"><div style="display:flex;justify-content:space-between"><label for="warn">${t("warnAt")}</label>
        <span class="mono tone-warn" style="font-weight:750" id="warn-val">${s.warn_at > 0 ? pct(s.warn_at) : t("off")}</span></div>
        <input id="warn" type="range" min="0" max="95" step="5" value="${s.warn_at}" data-set="warn_at"></div>
      <label class="item"><span>${t("whenReached")}</span><input type="checkbox" data-set="notify_full" ${s.notify_full ? "checked" : ""}></label>
      <label class="item"><span>${t("whenResets")}</span><input type="checkbox" data-set="notify_reset" ${s.notify_reset ? "checked" : ""}></label>
    </div></div>

    <div class="group"><span class="caps">${t("accounts")}</span><div class="card">${accounts}</div>
      <span class="faint" style="font-size:11px">${t("privacy")}</span></div>

    <div style="display:flex;justify-content:space-between;align-items:center;font-size:12px" class="faint">
      <span class="mono">v${esc(state.version)}${state.updateMsg ? ` · ${esc(state.updateMsg)}` : ""}</span>
      <button class="link" data-act="check-update">${t("checkUpdates")}</button></div>
  </div>`;
}

function render() {
  const scroll = app.querySelector(".body")?.scrollTop ?? 0;
  app.innerHTML =
    state.view === "details" ? details()
    : state.view === "settings" ? settingsView()
    : showWelcome() ? welcome()
    : overview();
  const body = app.querySelector<HTMLElement>(".body");
  if (body) body.scrollTop = scroll;
  requestAnimationFrame(fit);
}

/** Size the window to its content, so there's no empty space under the footer. */
let lastHeight = 0;
function fit() {
  const body = app.querySelector<HTMLElement>(".body");
  if (!body) return;
  const chrome = [...app.children].reduce((sum, el) => sum + (el === body ? 0 : (el as HTMLElement).offsetHeight), 0);
  // The body stretches to fill the window, so measure what's inside it.
  const kids = [...body.children] as HTMLElement[];
  const first = kids[0], last = kids[kids.length - 1];
  const content = first ? last.offsetTop + last.offsetHeight - first.offsetTop : 0;
  const style = getComputedStyle(body);
  const height = Math.ceil(chrome + content + parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) + 2); // + borders
  if (Math.abs(height - lastHeight) > 2) {
    lastHeight = height;
    invoke("fit_window", { height });
  }
}

function applyTheme() {
  if (state.settings.theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = state.settings.theme;
}

/** The webview knows the OS language, so "system" is resolved here and passed to Rust for notifications and the tray. */
function applyLanguage() {
  const lang = resolveLang(state.settings.language);
  setLang(lang);
  invoke("set_language", { lang });
}


async function refresh() {
  if (state.refreshing) return;
  state.refreshing = true;
  render();
  try {
    state.snap = await invoke<Snapshot>("refresh_now");
  } finally {
    state.refreshing = false;
    render();
  }
}

async function saveSettings(patch: Partial<Settings>) {
  state.settings = { ...state.settings, ...patch };
  applyTheme();
  if ("language" in patch) applyLanguage();
  await invoke("set_settings", { settings: state.settings });
}

/** Download a new version in the background; the banner offers the restart. */
async function checkUpdate(manual = false) {
  try {
    if (manual) {
      state.updateMsg = t("checking");
      render();
    }
    const update = await check();
    if (!update) {
      state.updateMsg = manual ? t("upToDate") : "";
    } else if (!state.updateReady) {
      state.update = update;
      state.updateMsg = t("downloading", { v: update.version });
      if (manual) render();
      await update.downloadAndInstall();
      state.updateReady = true;
      state.updateMsg = t("readyRestart", { v: update.version });
    }
  } catch {
    state.updateMsg = manual ? t("updateFailed") : "";
  }
  render();
}

app.addEventListener("click", async (e) => {
  const el = (e.target as HTMLElement).closest<HTMLElement>("[data-view],[data-act],[data-theme],[data-lang],[data-open],[data-copy]");
  if (!el) return;
  if (el.dataset.view) {
    state.view = el.dataset.view as View;
    render();
    app.querySelector(".body")?.scrollTo(0, 0);
  } else if (el.dataset.theme) {
    await saveSettings({ theme: el.dataset.theme as Settings["theme"] });
    render();
  } else if (el.dataset.lang) {
    await saveSettings({ language: el.dataset.lang as Settings["language"] });
    render();
  } else if (el.dataset.open) {
    await openUrl(USAGE_PAGES[el.dataset.open]);
  } else if (el.dataset.copy) {
    const text = el.dataset.copy;
    await copyText(text);
    state.copied = text;
    render();
    setTimeout(() => { if (state.copied === text) { state.copied = ""; render(); } }, 2000);
  } else if (el.dataset.act === "check-login") {
    state.welcomeMsg = "";
    await refresh();
    state.welcomeMsg = showWelcome() ? t("stillNoLogin") : "";
    render();
  } else if (el.dataset.act === "skip-welcome") {
    state.skipWelcome = true;
    render();
  } else if (el.dataset.act === "guide") {
    await openUrl(GUIDE + (lang() === "nl" ? "INSTALL.nl.md" : "INSTALL.md"));
  } else if (el.dataset.act === "refresh") refresh();
  else if (el.dataset.act === "quit") invoke("quit_app");
  else if (el.dataset.act === "restart") relaunch();
  else if (el.dataset.act === "check-update") checkUpdate(true);
});

app.addEventListener("input", (e) => {
  const el = e.target as HTMLInputElement;
  if (el.dataset.set === "warn_at") app.querySelector("#warn-val")!.textContent = Number(el.value) > 0 ? pct(Number(el.value)) : t("off");
});

app.addEventListener("change", async (e) => {
  const el = e.target as HTMLInputElement;
  const key = el.dataset.set;
  if (!key) return;
  if (key === "autostart") {
    try {
      el.checked ? await enable() : await disable();
      state.autostart = await isEnabled();
    } catch { el.checked = state.autostart; }
  } else if (key === "refresh_minutes" || key === "warn_at") {
    await saveSettings({ [key]: Number(el.value) });
  } else {
    await saveSettings({ [key]: el.checked });
  }
  render();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") state.view === "overview" ? invoke("hide_window") : ((state.view = "overview"), render());
});


async function start() {
  [state.snap, state.settings, state.version] = await Promise.all([
    invoke<Snapshot>("get_snapshot"),
    invoke<Settings>("get_settings"),
    getVersion(),
  ]);
  state.autostart = await isEnabled().catch(() => false);
  applyTheme();
  applyLanguage();
  render();

  await listen<Snapshot>("snapshot", (e) => { state.snap = e.payload; render(); });
  await listen<Settings>("settings", (e) => { state.settings = e.payload; applyTheme(); render(); });
  await listen("shown", () => { state.view = "overview"; render(); });
  // The OS language can change while we run; "system" should follow it.
  window.addEventListener("languagechange", () => { if (state.settings.language === "system") { applyLanguage(); render(); } });
  setInterval(render, 60_000); // relative times and countdowns

  checkUpdate();
  setInterval(checkUpdate, 6 * 3600 * 1000);
}

start();
