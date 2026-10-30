<script lang="ts">
  // Print-friendly timesheet (Stundenzettel): one row per day with start, end, breaks, worked, target.
  import type { ExportRow, Stats } from "../lib/api";
  import { formatDate, formatDuration, formatSigned, formatTime } from "../lib/format";

  let { stats, rows, user }: { stats: Stats; rows: ExportRow[]; user: string } = $props();

  const days = $derived(stats.days.filter((d) => d.seconds > 0 || d.targetSeconds > 0));
  function rowsFor(date: string) {
    return rows.filter((r) => r.date === date);
  }
</script>

<div class="sheet">
  <header>
    <h1>Stundenzettel</h1>
    <div class="meta">
      <div><strong>{user}</strong></div>
      <div>{formatDate(stats.from, { day: "2-digit", month: "2-digit", year: "numeric" })} – {formatDate(stats.to, { day: "2-digit", month: "2-digit", year: "numeric" })}</div>
      <div>Erstellt mit YaTTT am {new Date().toLocaleDateString("de-DE")}</div>
    </div>
  </header>

  <table class="summary">
    <thead>
      <tr>
        <th>Datum</th>
        <th>Beginn</th>
        <th>Ende</th>
        <th>Pausen</th>
        <th>Arbeitszeit</th>
        <th>Soll</th>
        <th>Differenz</th>
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
        <th colspan="3">Summe</th>
        <th class="num">{formatDuration(stats.breakSeconds)}</th>
        <th class="num">{formatDuration(stats.totalSeconds)}</th>
        <th class="num">{formatDuration(stats.targetSeconds)}</th>
        <th class="num">{formatSigned(stats.totalSeconds - stats.targetSeconds)}</th>
      </tr>
    </tfoot>
  </table>

  <h2>Einzelne Zeiten</h2>
  <table class="details">
    <thead>
      <tr>
        <th>Datum</th>
        <th>Von</th>
        <th>Bis</th>
        <th>Dauer</th>
        <th>Ticket</th>
        <th>Tätigkeit</th>
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
    <div class="sign">Datum, Unterschrift Mitarbeiter/in</div>
    <div class="sign">Datum, Unterschrift Vorgesetzte/r</div>
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
