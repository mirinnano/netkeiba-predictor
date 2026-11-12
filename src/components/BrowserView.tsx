import { useState } from "react";
import { ChevronLeft, ChevronRight, Loader2 } from "lucide-react";
import { KaisaiCalendarResponse, VenueRaces } from "../types";
import { Segmented } from "./Segmented";

export function BrowserView({
  isNar,
  calendar,
  selectedDate,
  venues,
  loading,
  onToggleNar,
  onLoadCalendar,
  onSelectDate,
  onPickRace,
  onBadDate,
}: {
  isNar: boolean;
  calendar: KaisaiCalendarResponse | null;
  selectedDate: string;
  venues: VenueRaces[];
  loading: boolean;
  onToggleNar: (nar: boolean) => void;
  onLoadCalendar: (date?: string) => void;
  onSelectDate: (date: string) => void;
  onPickRace: (raceId: string) => void;
  onBadDate: () => void;
}) {
  const [dateInput, setDateInput] = useState("");
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-[24px] font-black">開催一覧</h1>
        <div className="w-[220px]">
          <Segmented
            label="中央・地方"
            value={isNar ? "nar" : "jra"}
            options={[
              { value: "jra", label: "中央" },
              { value: "nar", label: "地方" },
            ]}
            onChange={(v) => onToggleNar(v === "nar")}
          />
        </div>
        {loading && <Loader2 className="h-4 w-4 animate-spin text-ink-muted" aria-label="読み込み中" />}
        <form
          className="ml-auto flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            const d = dateInput.trim();
            if (/^\d{8}$/.test(d)) onLoadCalendar(d);
            else onBadDate();
          }}
        >
          <input
            value={dateInput}
            onChange={(e) => setDateInput(e.target.value)}
            placeholder="YYYYMMDD"
            aria-label="開催日"
            inputMode="numeric"
            className="num h-8 w-32 rounded-md border border-line bg-paper px-2 text-[15px] placeholder:text-ink-faint focus:border-shell focus:outline-none"
          />
          <button type="submit" className="h-8 rounded-md border border-line bg-paper px-3 text-[12px] font-bold hover:border-ink">
            移動
          </button>
          <button
            type="button"
            onClick={() => onLoadCalendar()}
            className="h-8 rounded-md border border-line bg-paper px-3 text-[12px] font-bold hover:border-ink"
          >
            直近の開催
          </button>
        </form>
      </div>

      {calendar && (
        <nav aria-label="開催日" className="flex items-center gap-1 overflow-x-auto border-b border-ink pb-px">
          {calendar.prev_date && (
            <button
              type="button"
              onClick={() => onLoadCalendar(calendar.prev_date!)}
              className="flex items-center px-2 py-2 text-[12px] text-ink-muted hover:text-ink"
            >
              <ChevronLeft className="h-4 w-4" />前
            </button>
          )}
          {calendar.dates.map((d) => {
            const on = d.date === selectedDate;
            return (
              <button
                key={d.date}
                type="button"
                aria-current={on ? "date" : undefined}
                onClick={() => onSelectDate(d.date)}
                className={`-mb-px shrink-0 border-b-[3px] px-3 py-2 text-[13px] font-bold ${
                  on ? "border-shell text-ink" : "border-transparent text-ink-muted hover:text-ink"
                } ${d.is_sunday && !on ? "text-mark" : ""} ${d.is_saturday && !on ? "text-shell" : ""}`}
              >
                {d.label}
              </button>
            );
          })}
          {calendar.next_date && (
            <button
              type="button"
              onClick={() => onLoadCalendar(calendar.next_date!)}
              className="flex items-center px-2 py-2 text-[12px] text-ink-muted hover:text-ink"
            >
              次<ChevronRight className="h-4 w-4" />
            </button>
          )}
        </nav>
      )}

      {!loading && venues.length === 0 ? (
        <p className="py-10 text-center text-ink-muted">この日は開催がありません。別の日を選んでください。</p>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-x-6 gap-y-6">
          {venues.map((v) => (
            <section key={v.venue_name}>
              <header className="flex items-baseline justify-between border-b-2 border-ink pb-1">
                <h2 className="text-[17px] font-black">{v.short_venue}</h2>
                <span className="text-[11px] text-ink-muted">
                  {v.venue_name.replace(v.short_venue, "").trim()} ・ {v.weather}
                  {v.turf_cond && ` ・ 芝 ${v.turf_cond}`}
                  {v.dirt_cond && ` ・ ダ ${v.dirt_cond}`}
                </span>
              </header>
              <ul>
                {v.races.map((r) => (
                  <li key={r.race_id}>
                    <button
                      type="button"
                      onClick={() => onPickRace(r.race_id)}
                      className="grid w-full grid-cols-[40px_1fr_auto] items-center gap-2 border-b border-line-soft py-1.5 text-left hover:bg-paper"
                    >
                      <span className="num text-[18px] font-bold">{r.race_num.replace("R", "")}<span className="text-[11px] text-ink-muted">R</span></span>
                      <span className="min-w-0">
                        <span className="flex items-center gap-1.5">
                          <span className="truncate font-bold">{r.title}</span>
                          {r.grade && (
                            <span className="shrink-0 rounded-sm bg-ink px-1 text-[10px] font-black text-waku-5">{r.grade}</span>
                          )}
                        </span>
                        <span className="text-[11px] text-ink-muted">
                          {r.start_time} ・ {r.surface === "ダ" ? "ダート" : r.surface === "障" ? "障害" : "芝"}
                          {r.distance}m{r.head_count > 0 && ` ・ ${r.head_count}頭`}
                        </span>
                      </span>
                      <ChevronRight className="h-4 w-4 text-ink-faint" />
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
