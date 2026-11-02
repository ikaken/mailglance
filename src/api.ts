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
  refreshMessages: (email: string, view: View) =>
    invoke<Summary[]>("refresh_messages", { email, view }),
  getBody: (email: string, id: string) => invoke<MessageBody>("get_body", { email, id }),
  setUnread: (email: string, id: string, unread: boolean) =>
    invoke<string[]>("set_unread", { email, id, unread }),
  openLink: (email: string, url: string) => invoke<void>("open_link", { email, url }),
  openGmail: (email: string, url = "") => invoke<void>("open_gmail", { email, url }),
};

export const gmailUrls = {
  thread: (threadId: string) => `https://mail.google.com/mail/u/0/#all/${threadId}`,
  compose: () => "https://mail.google.com/mail/u/0/#inbox?compose=new",
  filters: () => "https://mail.google.com/mail/u/0/#settings/filters",
};

/** "山田 太郎 <taro@example.com>" → "山田 太郎" */
export function displayName(address: string): string {
  const first = address.split(",")[0].trim();
  const m = first.match(/^"?(.*?)"?\s*<(.+)>$/);
  if (m) return m[1] || m[2];
  return first;
}

export function formatDate(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString("ja-JP", { hour: "2-digit", minute: "2-digit" });
  }
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString("ja-JP", { month: "numeric", day: "numeric" });
  }
  return d.toLocaleDateString("ja-JP");
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
