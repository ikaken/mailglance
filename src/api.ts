import { invoke } from "@tauri-apps/api/core";

export interface AccountConfig {
  email: string;
  favorites: string[];
}

export interface AppInfo {
  configured: boolean;
  client_id: string;
  accounts: AccountConfig[];
}

export interface Label {
  id: string;
  name: string;
  kind: string;
  unread: number;
  total: number;
  color: string | null;
}

export interface Summary {
  id: string;
  thread_id: string;
  date: number;
  from: string;
  to: string;
  subject: string;
  snippet: string;
  label_ids: string[];
}

export interface MessagePage {
  messages: Summary[];
  next_page_token: string | null;
  estimate: number;
}

export interface Attachment {
  filename: string;
  size: number;
}

export interface MessageBody {
  html: string;
  attachments: Attachment[];
}

export type View = { kind: "label"; id: string } | { kind: "search"; q: string };

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  saveClient: (clientId: string, clientSecret: string) =>
    invoke<void>("save_client", { clientId, clientSecret }),
  addAccount: () => invoke<string>("add_account"),
  removeAccount: (email: string) => invoke<void>("remove_account", { email }),
  setFavorite: (email: string, labelId: string, on: boolean) =>
    invoke<void>("set_favorite", { email, labelId, on }),
  cachedLabels: (email: string) => invoke<Label[]>("cached_labels", { email }),
  refreshLabels: (email: string) => invoke<Label[]>("refresh_labels", { email }),
  cachedMessages: (email: string, view: View) =>
    invoke<Summary[]>("cached_messages", { email, view }),
  refreshMessages: (email: string, view: View, pageToken?: string) =>
    invoke<MessagePage>("refresh_messages", { email, view, pageToken: pageToken ?? null }),
  getBody: (email: string, id: string) => invoke<MessageBody>("get_body", { email, id }),
  modifyMessage: (email: string, id: string, add: string[], remove: string[]) =>
    invoke<string[]>("modify_message", { email, id, add, remove }),
  trashMessage: (email: string, id: string) => invoke<string[]>("trash_message", { email, id }),
  openLink: (email: string, url: string) => invoke<void>("open_link", { email, url }),
  openGmail: (email: string, url = "") => invoke<void>("open_gmail", { email, url }),
};

export const gmailUrls = {
  thread: (threadId: string) => `https://mail.google.com/mail/u/0/#all/${threadId}`,
  compose: () => "https://mail.google.com/mail/u/0/#inbox?compose=new",
  filters: () => "https://mail.google.com/mail/u/0/#settings/filters",
};

/** "山田 太郎 <taro@example.com>" → { name: "山田 太郎", address: "taro@example.com" } */
export function parseAddress(raw: string): { name: string; address: string } {
  const first = raw.split(",")[0].trim();
  const m = first.match(/^"?(.*?)"?\s*<(.+)>$/);
  if (m) return { name: m[1] || m[2], address: m[2] };
  return { name: first, address: first };
}

/**
 * Marketing mail pads its preview text with invisible characters (zero-width spaces,
 * combining grapheme joiners), which would otherwise push the ellipsis to odd places.
 */
export function cleanSnippet(s: string): string {
  return s
    .replace(/[\u00ad\u034f\u061c\u115f\u1160\u17b4\u17b5\u180e\u200b-\u200f\u2028-\u202f\u2060-\u206f\u3164\ufeff]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Gmail list style: time today, "11月2日" this year, "2024/03/05" before. */
export function formatListDate(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString("ja-JP", { hour: "2-digit", minute: "2-digit" });
  }
  if (d.getFullYear() === now.getFullYear()) return `${d.getMonth() + 1}月${d.getDate()}日`;
  return d.toLocaleDateString("ja-JP", { year: "numeric", month: "2-digit", day: "2-digit" });
}

/** "2026年11月3日(火) 14:05（2 時間前）" */
export function formatFullDate(ms: number): string {
  const d = new Date(ms);
  const text = d.toLocaleString("ja-JP", {
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
  const minutes = Math.round((Date.now() - ms) / 60000);
  let ago = "";
  if (minutes < 1) ago = "たった今";
  else if (minutes < 60) ago = `${minutes} 分前`;
  else if (minutes < 60 * 24) ago = `${Math.round(minutes / 60)} 時間前`;
  else if (minutes < 60 * 24 * 30) ago = `${Math.round(minutes / 60 / 24)} 日前`;
  return ago ? `${text}（${ago}）` : text;
}

const AVATAR_COLORS = ["#1a73e8", "#d93025", "#188038", "#e37400", "#9334e6", "#007b83", "#c5221f", "#b06000"];

/** Stable color per name, like Gmail's letter avatars. */
export function avatarColor(key: string): string {
  let h = 0;
  for (const c of key) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
