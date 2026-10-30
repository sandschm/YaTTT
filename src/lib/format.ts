import type { TimeFormat } from "./api";

export function formatDuration(seconds: number, format: TimeFormat = "hoursMinutes", withSeconds = false): string {
  const s = Math.max(0, Math.floor(seconds));
  if (format === "decimal") {
    return (s / 3600).toFixed(2).replace(".", ",") + " h";
  }
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (withSeconds) {
    return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  }
  return `${h}:${String(m).padStart(2, "0")}`;
}

export function formatSigned(seconds: number, format: TimeFormat = "hoursMinutes"): string {
  const sign = seconds < 0 ? "−" : "+";
  return `${sign}${formatDuration(Math.abs(seconds), format)}`;
}

export function formatTime(iso: string | Date): string {
  const d = typeof iso === "string" ? new Date(iso) : iso;
  return d.toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
}

export function formatDate(iso: string | Date, opts: Intl.DateTimeFormatOptions = { weekday: "short", day: "2-digit", month: "2-digit", year: "numeric" }): string {
  const d = typeof iso === "string" ? parseDate(iso) : iso;
  return d.toLocaleDateString("de-DE", opts);
}

/** Parses "YYYY-MM-DD" as a local date (not UTC). */
export function parseDate(s: string): Date {
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) {
    const [y, m, d] = s.split("-").map(Number);
    return new Date(y, m - 1, d);
  }
  return new Date(s);
}

/** "YYYY-MM-DD" of a local date. */
export function toDateKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

/** Monday-based start of week (firstDay: 0 = Monday … 6 = Sunday). */
export function startOfWeek(d: Date, firstDay = 0): Date {
  const day = (d.getDay() + 6) % 7; // 0 = Monday
  const diff = (day - firstDay + 7) % 7;
  return startOfDay(addDays(d, -diff));
}

export function relativeTime(iso: string | null): string {
  if (!iso) return "nie";
  const diff = Math.round((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 60) return "gerade eben";
  if (diff < 3600) return `vor ${Math.floor(diff / 60)} min`;
  if (diff < 86400) return `vor ${Math.floor(diff / 3600)} h`;
  return formatDate(iso, { day: "2-digit", month: "2-digit" });
}

/** "HH:MM" of a local time in a datetime-local input compatible format. */
export function toLocalInput(iso: string | Date): string {
  const d = typeof iso === "string" ? new Date(iso) : iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fromLocalInput(value: string): string {
  return new Date(value).toISOString();
}

/** Parse "1:30", "1,5", "90m", "1h 30m" into seconds. */
export function parseDurationInput(text: string): number | null {
  const t = text.trim().toLowerCase().replace(",", ".");
  if (!t) return null;
  let m = t.match(/^(\d+):(\d{1,2})$/);
  if (m) return Number(m[1]) * 3600 + Number(m[2]) * 60;
  m = t.match(/^(\d+(?:\.\d+)?)\s*h?$/);
  if (m) return Math.round(Number(m[1]) * 3600);
  m = t.match(/^(\d+)\s*m(?:in)?$/);
  if (m) return Number(m[1]) * 60;
  m = t.match(/^(\d+)\s*h\s*(\d+)\s*m?(?:in)?$/);
  if (m) return Number(m[1]) * 3600 + Number(m[2]) * 60;
  return null;
}
