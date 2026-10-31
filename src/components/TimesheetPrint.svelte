<script lang="ts">
  // Print-friendly timesheet: one row per day with start, end, breaks, worked, target.
  import type { ExportRow, Stats } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { formatDate, formatDuration, formatSigned, formatTime } from "../lib/format";

  let { stats, rows, user }: { stats: Stats; rows: ExportRow[]; user: string } = $props();

  const days = $derived(stats.days.filter((d) => d.seconds > 0 || d.targetSeconds > 0));
  const numeric: Intl.DateTimeFormatOptions = { day: "2-digit", month: "2-digit", year: "numeric" };
  function rowsFor(date: string) {
    return rows.filter((r) => r.date === date);
  }
</script>

<div class="sheet">
  <header>
    <h1>{t("sheet.title")}</h1>
    <div class="meta">
      <div><strong>{user}</strong></div>
      <div>{formatDate(stats.from, numeric)} – {formatDate(stats.to, numeric)}</div>
      <div>{t("sheet.createdWith", { date: formatDate(new Date(), numeric) })}</div>
    </div>
  </header>

  <table class="summary">
    <thead>
      <tr>
        <th>{t("sheet.date")}</th>
        <th>{t("sheet.begin")}</th>
        <th>{t("sheet.end")}</th>
        <th>{t("sheet.breaks")}</th>
        <th>{t("sheet.worked")}</th>
        <th>{t("sheet.target")}</th>
        <th>{t("sheet.diff")}</th>
      </tr>
    </thead>
    <tbody>
      {#each days as d (d.date)}
        <tr>
          <td>{formatDate(d.date)}</td>
          <td>{d.firstStart ? formatTime(d.firstStart) : "–"}</td>
          <td>{d.lastEnd ? formatTime(d.lastEnd) : "–"}</td>
          <td class="num">{formatDuration(d.breakSeconds)}</td>
          <td class="num">{formatDuration(d.seconds)}</td>
          <td class="num">{formatDuration(d.targetSeconds)}</td>
          <td class="num">{formatSigned(d.seconds - d.targetSeconds)}</td>
        </tr>
      {/each}
    </tbody>
    <tfoot>
      <tr>
        <th colspan="3">{t("sheet.sum")}</th>
        <th class="num">{formatDuration(stats.breakSeconds)}</th>
        <th class="num">{formatDuration(stats.totalSeconds)}</th>
        <th class="num">{formatDuration(stats.targetSeconds)}</th>
        <th class="num">{formatSigned(stats.totalSeconds - stats.targetSeconds)}</th>
      </tr>
    </tfoot>
  </table>

  <h2>{t("sheet.details")}</h2>
  <table class="details">
    <thead>
      <tr>
        <th>{t("sheet.date")}</th>
        <th>{t("sheet.from")}</th>
        <th>{t("sheet.to")}</th>
        <th>{t("sheet.duration")}</th>
        <th>{t("sheet.ticket")}</th>
        <th>{t("sheet.activity")}</th>
      </tr>
    </thead>
    <tbody>
      {#each days as d (d.date)}
        {#each rowsFor(d.date) as r (r.start + r.issueKey)}
          <tr>
            <td>{formatDate(r.date, { day: "2-digit", month: "2-digit" })}</td>
            <td>{formatTime(r.start)}</td>
            <td>{formatTime(r.end)}</td>
            <td class="num">{formatDuration(r.durationSeconds)}</td>
            <td>{r.issueKey}</td>
            <td>{r.summary}{r.note ? ` – ${r.note}` : ""}</td>
          </tr>
        {/each}
      {/each}
    </tbody>
  </table>

  <footer>
    <div class="sign">{t("sheet.signEmployee")}</div>
    <div class="sign">{t("sheet.signManager")}</div>
  </footer>
</div>

<style>
  .sheet {
    display: none;
    padding: 24px;
    color: #000;
    background: #fff;
    font-size: 11pt;
    font-family: var(--font);
  }
  @media print {
    .sheet {
      display: block;
    }
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 16px;
    border-bottom: 2px solid #000;
    padding-bottom: 8px;
  }
  h1 {
    margin: 0;
    font-size: 20pt;
  }
  h2 {
    font-size: 13pt;
    margin: 24px 0 8px;
  }
  .meta {
    text-align: right;
    font-size: 10pt;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 10pt;
  }
  th,
  td {
    border-bottom: 1px solid #999;
    padding: 4px 6px;
    text-align: left;
  }
  th {
    border-bottom: 2px solid #000;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  tfoot th {
    border-top: 2px solid #000;
    border-bottom: none;
  }
  footer {
    display: flex;
    gap: 40px;
    margin-top: 60px;
  }
  .sign {
    flex: 1;
    border-top: 1px solid #000;
    padding-top: 4px;
    font-size: 9pt;
  }
</style>
