import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, Loader2 } from "lucide-react";
import { RaceSummary, VenueRaces } from "../types";

type Row = RaceSummary & { venue: string; at: Date };

const ymd = (d: Date) => `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;

/** 当日のホーム: 今日の中央のレースを発走時刻順に並べ、次のレースまでの時間を出す。タップで予想する */
export function TodayView({ onPickRace, onOpenBrowser }: { onPickRace: (raceId: string) => void; onOpenBrowser: () => void }) {
  const [venues, setVenues] = useState<VenueRaces[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => new Date());
  const [venue, setVenue] = useState<string | null>(null);
  const [showDone, setShowDone] = useState(false);

  useEffect(() => {
    invoke<VenueRaces[]>("get_race_list_by_date", { kaisaiDate: ymd(new Date()), isNar: false })
      .then(setVenues)
      .catch((e) => setError(String(e)));
    const id = window.setInterval(() => setNow(new Date()), 30000);
    return () => window.clearInterval(id);
  }, []);

  const rows: Row[] = useMemo(() => {
    if (!venues) return [];
    const today = new Date();
    return venues
      .flatMap((v) =>
        v.races.map((r) => {
          const [h, m] = r.start_time.split(":").map(Number);
          const at = new Date(today.getFullYear(), today.getMonth(), today.getDate(), h || 0, m || 0);
          return { ...r, venue: v.short_venue || v.venue_name, at };
        }),
      )
      .sort((a, b) => a.at.getTime() - b.at.getTime());
  }, [venues]);

  const nextIdx = rows.findIndex((r) => r.at.getTime() > now.getTime());
  const venueNames = useMemo(() => (venues ?? []).map((v) => v.short_venue || v.venue_name), [venues]);

  if (error) {
    return <p className="text-[13px] text-ink-muted">今日のレースを取得できませんでした（{error}）</p>;
  }
  if (!venues) {
    return (
      <p className="flex items-center gap-2 text-[13px] text-ink-muted">
        <Loader2 className="h-4 w-4 animate-spin" />
        今日のレースを取得しています
      </p>
    );
  }
  if (rows.length === 0) {
    return (
      <div className="space-y-3">
        <h1 className="text-[24px] font-black">今日</h1>
        <p className="text-[13px] text-ink-muted">今日は中央競馬の開催がありません。</p>
        <button
          type="button"
          onClick={onOpenBrowser}
          className="h-11 rounded-md border border-line bg-paper px-4 text-[13px] font-bold text-ink-soft hover:border-ink"
        >
          開催一覧で次の開催を見る
        </button>
      </div>
    );
  }

  const inVenue = rows.map((r, i) => ({ r, i })).filter(({ r }) => !venue || r.venue === venue);
  const next = nextIdx >= 0 ? (inVenue.find(({ i }) => i >= nextIdx)?.r ?? null) : null;
  const doneCount = inVenue.filter(({ i }) => nextIdx === -1 || i < nextIdx).length;
  const shown = showDone ? inVenue : inVenue.filter(({ i }) => !(nextIdx === -1 || i < nextIdx));
  const mins = next ? Math.max(0, Math.round((next.at.getTime() - now.getTime()) / 60000)) : null;

  return (
    <div className="space-y-4">
      <div className="flex items-baseline justify-between gap-3">
        <h1 className="text-[24px] font-black">今日</h1>
        <span className="text-[12px] text-ink-muted">
          {venues.map((v) => v.short_venue || v.venue_name).join("・")} ・ {rows.length}レース
        </span>
      </div>

      {next && (
        <button
          type="button"
          onClick={() => onPickRace(next.race_id)}
          className="flex w-full items-center gap-4 rounded-lg bg-shell px-4 py-3 text-left text-white active:bg-shell-deep"
        >
          <div className="min-w-0 flex-1">
            <div className="text-[12px] text-shell-text">次のレース</div>
            <div className="truncate text-[20px] font-black leading-tight">
              {next.venue}
              {next.race_num} {next.title}
            </div>
            <div className="text-[12px] text-shell-text">
              {next.start_time}発走 ・ {next.surface === "ダ" ? "ダート" : next.surface}
              {next.distance}m ・ {next.head_count}頭
            </div>
          </div>
          <div className="shrink-0 text-right">
            {mins !== null && mins >= 60 ? (
              <div className="num text-[30px] font-bold leading-none">
                {Math.floor(mins / 60)}
                <span className="text-[14px] font-normal text-shell-text">時間</span>
                {mins % 60}
                <span className="text-[14px] font-normal text-shell-text">分後</span>
              </div>
            ) : (
              <div className="num text-[34px] font-bold leading-none">
                {mins}
                <span className="text-[14px] font-normal text-shell-text">分後</span>
              </div>
            )}
          </div>
        </button>
      )}

      {venueNames.length > 1 && (
        <div className="flex gap-1.5" role="group" aria-label="競馬場で絞る">
          {[null, ...venueNames].map((v) => (
            <button
              key={v ?? "all"}
              type="button"
              aria-pressed={venue === v}
              onClick={() => setVenue(v)}
              className={`h-9 rounded-full border px-4 text-[13px] font-bold transition-colors ${
                venue === v ? "border-ink bg-ink text-white" : "border-line bg-paper text-ink-soft"
              }`}
            >
              {v ?? "すべて"}
            </button>
          ))}
        </div>
      )}

      {doneCount > 0 && (
        <button
          type="button"
          onClick={() => setShowDone((s) => !s)}
          className="w-full text-left text-[12px] font-bold text-ink-muted"
        >
          {showDone ? "発走済みのレースを隠す" : `発走済み ${doneCount} レースを表示`}
        </button>
      )}

      <ul className="divide-y divide-line-soft border-y border-ink">
        {shown.map(({ r, i }) => {
          const done = nextIdx === -1 || i < nextIdx;
          return (
            <li key={r.race_id}>
              <button
                type="button"
                onClick={() => onPickRace(r.race_id)}
                className={`flex min-h-14 w-full items-center gap-3 py-2 text-left active:bg-paper ${done ? "text-ink-faint" : ""}`}
              >
                <span className={`num w-12 shrink-0 text-[17px] font-bold ${done ? "" : "text-ink"}`}>{r.start_time}</span>
                <span className="w-16 shrink-0 text-[12px] text-ink-muted">
                  {r.venue}
                  <span className="num ml-0.5 text-[15px] font-bold text-ink-soft">{r.race_num}</span>
                </span>
                <span className="min-w-0 flex-1">
                  <span className={`block truncate text-[14px] font-bold ${done ? "" : "text-ink"}`}>
                    {r.title}
                    {r.grade && (
                      <span className="ml-1.5 rounded-sm bg-ink px-1 py-px align-middle text-[10px] font-black text-waku-5">{r.grade}</span>
                    )}
                  </span>
                  <span className="block text-[11px] text-ink-muted">
                    {r.surface === "ダ" ? "ダート" : r.surface}
                    {r.distance}m ・ {r.head_count}頭{done ? " ・ 発走済み" : ""}
                  </span>
                </span>
                <ChevronRight className="h-4 w-4 shrink-0 text-ink-faint" />
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
