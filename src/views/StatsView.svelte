<script lang="ts">
  import { tick } from "svelte";
  import { store } from "../lib/store.svelte";
  import { api, isTauri, type ExportRow, type Stats } from "../lib/api";
  import { addDays, formatDate, formatDuration, formatSigned, parseDate, startOfDay, startOfWeek, toDateKey } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import TimesheetPrint from "../components/TimesheetPrint.svelte";

  type Preset = "thisWeek" | "lastWeek" | "thisMonth" | "lastMonth" | "thisYear" | "custom";
  let preset = $state<Preset>("thisWeek");
  let customFrom = $state(toDateKey(addDays(new Date(), -30)));
  let customTo = $state(toDateKey(new Date()));
  let stats = $state<Stats | null>(null);
  let heat = $state<Stats | null>(null);
  let summarize = $state(true);
  let printing = $state(false);
  let printRows = $state<ExportRow[]>([]);
  let exporting = $state(false);
  let chartWidth = $state(600);

  const firstDay = $derived(store.settings?.work.firstDayOfWeek ?? 0);

  const range = $derived.by((): { from: string; to: string } => {
    const today = startOfDay(new Date());
    switch (preset) {
      case "thisWeek": {
        const s = startOfWeek(today, firstDay);
        return { from: toDateKey(s), to: toDateKey(addDays(s, 6)) };
      }
      case "lastWeek": {
        const s = addDays(startOfWeek(today, firstDay), -7);
        return { from: toDateKey(s), to: toDateKey(addDays(s, 6)) };
      }
      case "thisMonth":
        return { from: toDateKey(new Date(today.getFullYear(), today.getMonth(), 1)), to: toDateKey(new Date(today.getFullYear(), today.getMonth() + 1, 0)) };
      case "lastMonth":
        return { from: toDateKey(new Date(today.getFullYear(), today.getMonth() - 1, 1)), to: toDateKey(new Date(today.getFullYear(), today.getMonth(), 0)) };
      case "thisYear":
        return { from: toDateKey(new Date(today.getFullYear(), 0, 1)), to: toDateKey(new Date(today.getFullYear(), 11, 31)) };
      default:
        return { from: customFrom, to: customTo <= customFrom ? customFrom : customTo };
    }
  });

  $effect(() => {
    void store.entriesVersion;
    const { from, to } = range;
    api.getStats(from, to).then((s) => (stats = s));
  });

  $effect(() => {
    void store.entriesVersion;
    const to = startOfDay(new Date());
    const from = addDays(to, -364);
    api.getStats(toDateKey(from), toDateKey(to)).then((s) => (heat = s));
  });

  const tf = $derived(store.timeFormat);
  const balance = $derived(stats ? stats.totalSeconds - stats.targetSeconds : 0);
  const billablePct = $derived(stats && stats.totalSeconds ? Math.round((stats.billableSeconds / stats.totalSeconds) * 100) : 0);

  // ---- bar chart data: daily, or weekly when the range is long
  interface Bar {
    label: string;
    seconds: number;
    target: number;
    full: string;
  }
  const bars = $derived.by((): Bar[] => {
    if (!stats) return [];
    if (stats.days.length <= 31) {
      return stats.days.map((d) => ({
        label: stats!.days.length <= 7 ? formatDate(d.date, { weekday: "short" }) : String(parseDate(d.date).getDate()),
        seconds: d.seconds,
        target: d.targetSeconds,
        full: formatDate(d.date),
      }));
    }
    const weeks = new Map<string, Bar>();
    for (const d of stats.days) {
      const ws = startOfWeek(parseDate(d.date), firstDay);
      const k = toDateKey(ws);
      const bar = weeks.get(k) ?? { label: `KW ${isoWeek(ws)}`, seconds: 0, target: 0, full: `Woche ab ${formatDate(ws, { day: "2-digit", month: "2-digit" })}` };
      bar.seconds += d.seconds;
      bar.target += d.targetSeconds;
      weeks.set(k, bar);
    }
    return [...weeks.values()];
  });
  const barMax = $derived(Math.max(3600, ...bars.map((b) => Math.max(b.seconds, b.target))));

  function isoWeek(d: Date): number {
    const date = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
    const day = date.getUTCDay() || 7;
    date.setUTCDate(date.getUTCDate() + 4 - day);
    const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
    return Math.ceil(((date.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
  }

  // ---- donut
  const donut = $derived.by(() => {
    if (!stats || !stats.totalSeconds) return [];
    const r = 46;
    const c = 2 * Math.PI * r;
    let offset = 0;
    return stats.byProject.map((p) => {
      const frac = p.seconds / stats!.totalSeconds;
      const seg = { ...p, frac, dash: `${frac * c} ${c - frac * c}`, offset: -offset * c, color: p.color ?? store.color(p.key) };
      offset += frac;
      return seg;
    });
  });

  // ---- heatmap
  const heatCells = $derived.by(() => {
    if (!heat) return { weeks: [] as { date: string; seconds: number; level: number }[][], months: [] as { label: string; col: number }[] };
    const max = Math.max(1, ...heat.days.map((d) => d.seconds));
    const weeks: { date: string; seconds: number; level: number }[][] = [];
    let current: { date: string; seconds: number; level: number }[] = [];
    const months: { label: string; col: number }[] = [];
    let lastMonth = -1;
    const firstWeekday = (parseDate(heat.days[0].date).getDay() + 6) % 7;
    for (let i = 0; i < firstWeekday; i++) current.push({ date: "", seconds: 0, level: -1 });
    for (const d of heat.days) {
      const date = parseDate(d.date);
      if (date.getMonth() !== lastMonth) {
        months.push({ label: date.toLocaleDateString("de-DE", { month: "short" }), col: weeks.length });
        lastMonth = date.getMonth();
      }
      const level = d.seconds === 0 ? 0 : Math.min(4, Math.ceil((d.seconds / max) * 4));
      current.push({ date: d.date, seconds: d.seconds, level });
      if (current.length === 7) {
        weeks.push(current);
        current = [];
      }
    }
    if (current.length) weeks.push(current);
    return { weeks, months };
  });

  // ---- export
  async function exportFile(format: "csv" | "json") {
    exporting = true;
    try {
      const name = `yattt-${range.from}_${range.to}.${format}`;
      if (isTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: name, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
        if (!path) return;
        const n = await api.exportEntries(range.from, range.to, format, summarize, path);
        store.toast(`${n} Zeilen exportiert`, "success");
      } else {
        const rows = await api.getExportRows(range.from, range.to, summarize);
        const content = format === "json" ? JSON.stringify(rows, null, 2) : rowsToCsv(rows);
        const blob = new Blob([content], { type: format === "json" ? "application/json" : "text/csv;charset=utf-8" });
        const a = document.createElement("a");
        a.href = URL.createObjectURL(blob);
        a.download = name;
        a.click();
        URL.revokeObjectURL(a.href);
      }
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      exporting = false;
    }
  }

  function rowsToCsv(rows: ExportRow[]): string {
    const head = ["Datum", "Start", "Ende", "Dauer", "Dauer (Sekunden)", "Projekt", "Ticket", "Titel", "Notiz", "Abrechenbar", "Buchungsstatus", "Jira-Worklog-ID"];
    const esc = (v: string) => (/[;"\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
    const lines = rows.map((r) =>
      [
        formatDate(r.date, { day: "2-digit", month: "2-digit", year: "numeric" }),
        new Date(r.start).toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" }),
        new Date(r.end).toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" }),
        formatDuration(r.durationSeconds, tf),
        String(Math.round(r.durationSeconds)),
        r.projectKey,
        r.issueKey,
        r.summary,
        r.note,
        r.billable ? "ja" : "nein",
        r.syncState,
        r.jiraWorklogId ?? "",
      ]
        .map(esc)
        .join(";"),
    );
    return "﻿" + [head.join(";"), ...lines].join("\n");
  }

  async function printTimesheet() {
    printRows = await api.getExportRows(range.from, range.to, false);
    printing = true;
    await tick();
    const done = () => {
      printing = false;
      window.removeEventListener("afterprint", done);
    };
    window.addEventListener("afterprint", done);
    setTimeout(() => window.print(), 50);
  }
</script>

{#if printing && stats}
  <TimesheetPrint {stats} rows={printRows} user={store.auth.user?.displayName ?? ""} />
{/if}

<div class="view" class:no-print={printing}>
  <div class="view-header">
    <h1>Statistik</h1>
    <div class="seg">
      {#each [["thisWeek", "Diese Woche"], ["lastWeek", "Letzte Woche"], ["thisMonth", "Dieser Monat"], ["lastMonth", "Letzter Monat"], ["thisYear", "Dieses Jahr"], ["custom", "Zeitraum"]] as [id, label] (id)}
        <button class:on={preset === id} type="button" onclick={() => (preset = id as Preset)}>{label}</button>
      {/each}
    </div>
    {#if preset === "custom"}
      <input type="date" bind:value={customFrom} />
      <span class="faint">bis</span>
      <input type="date" bind:value={customTo} />
    {:else}
      <span class="muted">{formatDate(range.from, { day: "2-digit", month: "2-digit", year: "numeric" })} – {formatDate(range.to, { day: "2-digit", month: "2-digit", year: "numeric" })}</span>
    {/if}
  </div>

  <div class="view-body">
    {#if stats}
      <div class="kpis">
        <div class="kpi card">
          <span class="label">Erfasst</span>
          <span class="value mono">{formatDuration(stats.totalSeconds, tf)}</span>
          <span class="sub faint">{stats.trackedDays} {stats.trackedDays === 1 ? "Tag" : "Tage"} mit Zeiten</span>
        </div>
        <div class="kpi card">
          <span class="label">Soll</span>
          <span class="value mono">{formatDuration(stats.targetSeconds, tf)}</span>
          <span class="sub faint">{stats.workDays} Arbeitstage</span>
        </div>
        <div class="kpi card" class:pos={balance >= 0} class:neg={balance < 0}>
          <span class="label">Saldo</span>
          <span class="value mono">{formatSigned(balance, tf)}</span>
          <span class="sub faint">{balance >= 0 ? "Überstunden" : "Unterstunden"}</span>
        </div>
        <div class="kpi card">
          <span class="label">Abrechenbar</span>
          <span class="value mono">{billablePct}%</span>
          <span class="sub faint">{formatDuration(stats.billableSeconds, tf)} h</span>
        </div>
        <div class="kpi card">
          <span class="label">Pausen</span>
          <span class="value mono">{formatDuration(stats.breakSeconds, tf)}</span>
          <span class="sub faint">Ø {formatDuration(stats.trackedDays ? stats.breakSeconds / stats.trackedDays : 0, tf)} pro Tag</span>
        </div>
        <div class="kpi card">
          <span class="label">Ø pro Tag</span>
          <span class="value mono">{formatDuration(stats.avgPerTrackedDaySeconds, tf)}</span>
          <span class="sub faint">an Tagen mit Zeiten</span>
        </div>
        <div class="kpi card">
          <span class="label">Nach Jira gebucht</span>
          <span class="value mono">{stats.totalSeconds ? Math.round((stats.bookedSeconds / stats.totalSeconds) * 100) : 0}%</span>
          <span class="sub faint">{formatDuration(stats.unbookedSeconds, tf)} h offen</span>
        </div>
      </div>

      <div class="grid">
        <section class="card panel wide">
          <h3>Stunden pro {bars.length === stats.days.length ? "Tag" : "Woche"}</h3>
          <div class="chart-wrap" bind:clientWidth={chartWidth}>
          <svg class="bars" viewBox="0 0 {chartWidth} 160" role="img" aria-label="Stunden pro Tag">
            {#each bars as b, i (i)}
              {@const w = chartWidth / bars.length}
              {@const h = (b.seconds / barMax) * 120}
              {@const th = (b.target / barMax) * 120}
              <g>
                <title>{b.full}: {formatDuration(b.seconds, tf)} h{b.target ? ` (Soll ${formatDuration(b.target, tf)})` : ""}</title>
                <rect x={i * w + w * 0.2} y={135 - h} width={w * 0.6} height={h} rx="3" fill={b.target && b.seconds >= b.target ? "var(--success)" : "var(--accent)"} />
                {#if b.target}
                  <line x1={i * w + w * 0.12} x2={i * w + w * 0.88} y1={135 - th} y2={135 - th} stroke="var(--text-3)" stroke-dasharray="3 2" />
                {/if}
                <text x={i * w + w / 2} y="152" text-anchor="middle" font-size="10" fill="var(--text-3)">{b.label}</text>
              </g>
            {/each}
          </svg>
          </div>
        </section>

        <section class="card panel">
          <h3>Verteilung nach Projekt</h3>
          {#if donut.length}
            <div class="donut-wrap">
              <svg viewBox="0 0 120 120" class="donut" role="img" aria-label="Verteilung nach Projekt">
                {#each donut as seg (seg.key)}
                  <circle cx="60" cy="60" r="46" fill="none" stroke={seg.color} stroke-width="16" stroke-dasharray={seg.dash} stroke-dashoffset={seg.offset} transform="rotate(-90 60 60)">
                    <title>{seg.label}: {formatDuration(seg.seconds, tf)} h ({Math.round(seg.frac * 100)}%)</title>
                  </circle>
                {/each}
                <text x="60" y="64" text-anchor="middle" font-size="13" font-weight="600" fill="var(--text)">{formatDuration(stats.totalSeconds, tf)}</text>
              </svg>
              <ul class="legend">
                {#each donut as seg (seg.key)}
                  <li>
                    <span class="dot" style="background:{seg.color}"></span>
                    <span class="ellipsis grow">{seg.label}</span>
                    <span class="mono">{Math.round(seg.frac * 100)}%</span>
                    <span class="mono muted">{formatDuration(seg.seconds, tf)}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {:else}
            <p class="faint">Keine Zeiten im Zeitraum.</p>
          {/if}
        </section>

        <section class="card panel">
          <h3>Top-Tickets</h3>
          {#if stats.byIssue.length}
            <ul class="issues">
              {#each stats.byIssue.slice(0, 12) as g (g.key)}
                <li>
                  <div class="row">
                    <span class="dot" style="background:{g.color ?? store.color(g.projectKey ?? '')}"></span>
                    <span class="mono key">{g.key}</span>
                    <span class="ellipsis grow">{g.label}</span>
                    <span class="mono">{formatDuration(g.seconds, tf)}</span>
                  </div>
                  <div class="bar"><div class="fill" style="width:{(g.seconds / stats.byIssue[0].seconds) * 100}%; background:{g.color ?? store.color(g.projectKey ?? '')}"></div></div>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="faint">Keine Zeiten im Zeitraum.</p>
          {/if}
        </section>

        <section class="card panel wide">
          <h3>Aktivität der letzten 12 Monate</h3>
          {#if heat}
            <div class="heat-scroll">
              <svg viewBox="0 0 {heatCells.weeks.length * 13 + 30} 110" class="heat" role="img" aria-label="Aktivität pro Tag">
                {#each ["Mo", "Mi", "Fr"] as label, i (label)}
                  <text x="0" y={i * 26 + 22} font-size="9" fill="var(--text-3)">{label}</text>
                {/each}
                {#each heatCells.months as m (m.label + m.col)}
                  <text x={m.col * 13 + 28} y="8" font-size="9" fill="var(--text-3)">{m.label}</text>
                {/each}
                {#each heatCells.weeks as week, wi (wi)}
                  {#each week as cell, di (di)}
                    {#if cell.level >= 0}
                      <rect x={wi * 13 + 28} y={di * 13 + 13} width="11" height="11" rx="2" class="cell l{cell.level}">
                        <title>{formatDate(cell.date)}: {formatDuration(cell.seconds, tf)} h</title>
                      </rect>
                    {/if}
                  {/each}
                {/each}
              </svg>
            </div>
          {/if}
        </section>

        <section class="card panel wide export">
          <h3>Export</h3>
          <div class="row wrap">
            <label class="check" style="margin:0">
              <input type="checkbox" bind:checked={summarize} />
              <span>Pro Tag und Ticket zusammenfassen</span>
            </label>
            <span class="grow"></span>
            <button class="btn" type="button" onclick={() => exportFile("csv")} disabled={exporting}><Icon name="download" size={15} /> CSV</button>
            <button class="btn" type="button" onclick={() => exportFile("json")} disabled={exporting}><Icon name="download" size={15} /> JSON</button>
            <button class="btn" type="button" onclick={printTimesheet}><Icon name="print" size={15} /> Stundenzettel (PDF/Druck)</button>
          </div>
          <p class="faint small">Der Stundenzettel enthält pro Tag Beginn, Ende, Pausen, Arbeitszeit und Soll/Ist. Über den Druckdialog lässt er sich als PDF sichern.</p>
        </section>
      </div>
    {:else}
      <p class="faint">Lade …</p>
    {/if}
  </div>
</div>

<style>
  .small {
    font-size: 12px;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    border: none;
    background: var(--surface);
    padding: 5px 10px;
    cursor: pointer;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .kpis {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
    margin-bottom: 14px;
  }
  .kpi {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .kpi .label {
    font-size: 12px;
    color: var(--text-2);
  }
  .kpi .value {
    font-size: 20px;
    font-weight: 600;
  }
  .kpi.pos .value {
    color: var(--success);
  }
  .kpi.neg .value {
    color: var(--danger);
  }
  .kpi .sub {
    font-size: 11.5px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .panel {
    padding: 14px 16px;
    min-width: 0;
  }
  .panel.wide {
    grid-column: 1 / -1;
  }
  .panel h3 {
    margin: 0 0 10px;
    font-size: 13.5px;
    color: var(--text-2);
    font-weight: 600;
  }
  .chart-wrap {
    width: 100%;
  }
  .bars {
    width: 100%;
    height: 180px;
    display: block;
  }
  .donut-wrap {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .donut {
    width: 140px;
    height: 140px;
    flex-shrink: 0;
  }
  .legend,
  .issues {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .legend li {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12.5px;
  }
  .issues li {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12.5px;
  }
  .issues .key {
    color: var(--text-2);
  }
  .bar {
    height: 4px;
    background: var(--surface-3);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 2px;
  }
  .heat-scroll {
    overflow-x: auto;
  }
  .heat {
    min-width: 720px;
    width: 100%;
    height: 120px;
  }
  .cell {
    fill: var(--surface-3);
  }
  .cell.l1 {
    fill: color-mix(in srgb, var(--accent) 30%, var(--surface-3));
  }
  .cell.l2 {
    fill: color-mix(in srgb, var(--accent) 55%, var(--surface-3));
  }
  .cell.l3 {
    fill: color-mix(in srgb, var(--accent) 80%, var(--surface-3));
  }
  .cell.l4 {
    fill: var(--accent);
  }
  .wrap {
    flex-wrap: wrap;
  }
  .export p {
    margin: 8px 0 0;
  }
  @media (max-width: 900px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
