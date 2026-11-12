import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, Search, X } from "lucide-react";
import {
  IngestReport,
  JvStatus,
  KaisaiCalendarResponse,
  RacePredictionResult,
  RoiDashboardStats,
  ScoringWeights,
  VenueRaces,
} from "./types";
import { RaceView } from "./components/RaceView";
import { BetsView } from "./components/BetsView";
import { BrowserView } from "./components/BrowserView";
import { HistoryView } from "./components/HistoryView";
import { PRESETS } from "./components/WeightsPanel";
import { JvDataPanel } from "./components/JvDataPanel";

type Tab = "race" | "browser" | "bets" | "history";

const TABS: { id: Tab; label: string }[] = [
  { id: "race", label: "出馬表" },
  { id: "browser", label: "開催一覧" },
  { id: "bets", label: "買い目" },
  { id: "history", label: "回収率" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("browser");
  const [raceInput, setRaceInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [busyText, setBusyText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [prediction, setPrediction] = useState<RacePredictionResult | null>(null);
  const [weights, setWeights] = useState<ScoringWeights>(PRESETS[0].w);
  const [budget, setBudget] = useState(3000);
  const [strategy, setStrategy] = useState("balance");
  const [memo, setMemo] = useState("");

  const [isNar, setIsNar] = useState(false);
  const [calendar, setCalendar] = useState<KaisaiCalendarResponse | null>(null);
  const [selectedDate, setSelectedDate] = useState("");
  const [venues, setVenues] = useState<VenueRaces[]>([]);
  const [browserLoading, setBrowserLoading] = useState(false);

  const [dashboard, setDashboard] = useState<RoiDashboardStats | null>(null);
  const [jvStatus, setJvStatus] = useState<JvStatus | null>(null);

  // 再計算は連続で呼ばれるので、最後に投げたものだけを反映する
  const recalcSeq = useRef(0);
  const sliderTimer = useRef<number | undefined>(undefined);

  const showToast = (msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast((cur) => (cur === msg ? null : cur)), 3500);
  };

  useEffect(() => {
    refreshDashboard();
    refreshJvStatus();
    loadCalendar(undefined, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshJvStatus = async () => {
    try {
      setJvStatus(await invoke<JvStatus>("jv_status"));
    } catch (e) {
      setError(String(e));
    }
  };

  const importJv = async (dir: string): Promise<IngestReport[] | null> => {
    try {
      const r = await invoke<IngestReport[]>("jv_import", { dir });
      await refreshJvStatus();
      showToast("JRA-VAN データを取り込みました");
      return r;
    } catch (e) {
      setError(String(e));
      return null;
    }
  };

  const refreshDashboard = async () => {
    try {
      setDashboard(await invoke<RoiDashboardStats>("get_roi_dashboard"));
    } catch (e) {
      setError(String(e));
    }
  };

  const loadCalendar = async (date?: string, nar = isNar) => {
    setBrowserLoading(true);
    setError(null);
    try {
      const cal = await invoke<KaisaiCalendarResponse>("get_kaisai_calendar", {
        targetDate: date || null,
        isNar: nar,
      });
      setCalendar(cal);
      const d = date || cal.current_target;
      setSelectedDate(d);
      setVenues(await invoke<VenueRaces[]>("get_race_list_by_date", { kaisaiDate: d, isNar: nar }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBrowserLoading(false);
    }
  };

  const selectDate = async (d: string) => {
    setSelectedDate(d);
    setBrowserLoading(true);
    setError(null);
    try {
      setVenues(await invoke<VenueRaces[]>("get_race_list_by_date", { kaisaiDate: d, isNar }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBrowserLoading(false);
    }
  };

  const analyze = async (input?: string) => {
    const q = (input ?? raceInput).trim();
    if (!q) return;
    setRaceInput(q);
    setBusy(true);
    setBusyText("netkeibaから出馬表・過去5走・オッズを取得しています");
    setError(null);
    try {
      const res = await invoke<RacePredictionResult>("analyze_race", { input: q, weights, budget, strategy });
      setPrediction(res);
      setMemo(res.memo || "");
      setTab("race");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setBusyText("");
    }
  };

  const recalc = async (w: ScoringWeights, b: number, s: string) => {
    if (!prediction) return;
    const seq = ++recalcSeq.current;
    try {
      const updated = await invoke<RacePredictionResult>("recalculate_prediction", {
        currentResult: prediction,
        weights: w,
        budget: b,
        strategy: s,
        memo,
      });
      if (seq === recalcSeq.current) setPrediction(updated);
    } catch (e) {
      setError(String(e));
    }
  };

  const changeWeights = (w: ScoringWeights) => {
    setWeights(w);
    window.clearTimeout(sliderTimer.current);
    sliderTimer.current = window.setTimeout(() => recalc(w, budget, strategy), 180);
  };

  const save = async () => {
    if (!prediction) return;
    setBusy(true);
    try {
      const saved = await invoke<RacePredictionResult>("save_prediction_record", {
        result: { ...prediction, memo },
      });
      setPrediction(saved);
      await refreshDashboard();
      showToast(
        saved.pre_race
          ? `${saved.race_info.race_name} の予想を保存しました`
          : `${saved.race_info.race_name} を保存しました（確定後の予想なので回収率には含めません）`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const verify = async (raceId: string) => {
    setBusy(true);
    setBusyText("確定結果と払戻を取得しています");
    setError(null);
    try {
      const updated = await invoke<RacePredictionResult>("verify_saved_race_result", { raceId });
      await refreshDashboard();
      showToast(
        updated.race_info.is_finished
          ? `保存した買い目を照合しました（払戻 ${(updated.total_return ?? 0).toLocaleString()}円）`
          : "まだ確定していません。保存内容は変えていません",
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setBusyText("");
    }
  };

  const openSaved = async (raceId: string) => {
    try {
      const loaded = await invoke<RacePredictionResult>("load_prediction_record", { raceId });
      setPrediction(loaded);
      setWeights(loaded.weights);
      setBudget(loaded.budget);
      setStrategy(loaded.strategy);
      setMemo(loaded.memo);
      setRaceInput(loaded.race_info.race_id);
      setTab("race");
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (raceId: string) => {
    try {
      setDashboard(await invoke<RoiDashboardStats>("delete_prediction_record", { raceId }));
      showToast("保存した予想を削除しました");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-30 bg-shell text-white">
        <div className="mx-auto flex h-14 max-w-[1600px] items-center gap-6 px-6">
          <span className="text-[17px] font-black tracking-wide">KeibaScope</span>
          <nav aria-label="画面" className="flex gap-1">
            {TABS.map((t) => {
              const disabled = (t.id === "race" || t.id === "bets") && !prediction;
              return (
                <button
                  key={t.id}
                  type="button"
                  disabled={disabled}
                  aria-current={tab === t.id ? "page" : undefined}
                  onClick={() => {
                    setTab(t.id);
                    if (t.id === "history") refreshDashboard();
                  }}
                  className={`rounded-md px-3.5 py-2 text-[13px] font-bold transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
                    tab === t.id ? "bg-paper text-shell" : "text-shell-text hover:bg-shell-deep hover:text-white"
                  }`}
                >
                  {t.label}
                  {t.id === "bets" && prediction && (
                    <span className="num ml-1.5 text-[14px] opacity-70">
                      {prediction.recommendations.filter((r) => r.suggested_amount > 0).length}
                    </span>
                  )}
                </button>
              );
            })}
          </nav>
          <form
            className="ml-auto flex w-[460px] max-w-[45%] items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              analyze();
            }}
          >
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-shell-text" />
              <input
                value={raceInput}
                onChange={(e) => setRaceInput(e.target.value)}
                placeholder="レースID（12桁）か netkeiba のURL"
                aria-label="レースIDまたはnetkeibaのURL"
                spellCheck={false}
                className="num h-9 w-full rounded-md border border-shell-edge bg-shell-deep pl-8 pr-2 text-[15px] text-white placeholder:font-sans placeholder:text-[12px] placeholder:text-shell-text focus:border-white focus:outline-none"
              />
            </div>
            <button
              type="submit"
              disabled={busy || !raceInput.trim()}
              className="flex h-9 items-center gap-1.5 rounded-md bg-paper px-4 text-[13px] font-bold text-shell hover:bg-ground disabled:opacity-50"
            >
              {busy && <Loader2 className="h-4 w-4 animate-spin" />}
              予想する
            </button>
          </form>
        </div>
        {busy && busyText && (
          <div className="h-0.5 overflow-hidden bg-shell-deep" role="progressbar" aria-label={busyText}>
            <div className="h-full w-1/3 animate-[progress_1.2s_ease-in-out_infinite] bg-waku-5" />
          </div>
        )}
      </header>

      <main className="mx-auto max-w-[1600px] px-6 py-5">
        {busy && busyText && <p className="mb-3 text-[12px] text-ink-muted">{busyText}…</p>}
        {error && (
          <div role="alert" className="mb-4 flex items-start gap-3 rounded-md border border-mark/40 bg-mark/5 px-4 py-2.5 text-[13px]">
            <span className="font-bold text-mark">エラー</span>
            <span className="text-ink-soft">{error}</span>
            <button
              type="button"
              onClick={() => setError(null)}
              aria-label="閉じる"
              className="ml-auto text-ink-muted hover:text-ink"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        )}

        {tab === "race" && prediction && (
          <RaceView
            result={prediction}
            weights={weights}
            budget={budget}
            strategy={strategy}
            onWeights={changeWeights}
            onPreset={(w) => {
              setWeights(w);
              recalc(w, budget, strategy);
            }}
            onBudget={(b) => {
              setBudget(b);
              recalc(weights, b, strategy);
            }}
            onStrategy={(s) => {
              setStrategy(s);
              recalc(weights, budget, s);
            }}
            onSave={save}
            onRefresh={() => analyze(prediction.race_info.race_id)}
            busy={busy}
            memo={memo}
            onMemo={setMemo}
          />
        )}
        {tab === "bets" && prediction && <BetsView result={prediction} />}
        {tab === "browser" && (
          <BrowserView
            isNar={isNar}
            calendar={calendar}
            selectedDate={selectedDate}
            venues={venues}
            loading={browserLoading}
            onToggleNar={(nar) => {
              setIsNar(nar);
              loadCalendar(undefined, nar);
            }}
            onLoadCalendar={(d) => loadCalendar(d)}
            onSelectDate={selectDate}
            onPickRace={(id) => analyze(id)}
            onBadDate={() => setError("開催日は YYYYMMDD の8桁で入力してください（例: 20261115）")}
          />
        )}
        {tab === "history" && (
          <div className="mb-5">
            <JvDataPanel status={jvStatus} onImport={importJv} />
          </div>
        )}
        {tab === "history" && dashboard && (
          <HistoryView
            stats={dashboard}
            busy={busy}
            onReload={refreshDashboard}
            onOpen={openSaved}
            onVerify={verify}
            onDelete={remove}
          />
        )}
      </main>

      {toast && (
        <div
          role="status"
          className="fixed bottom-5 right-5 z-40 rounded-md bg-ink px-4 py-2.5 text-[13px] text-white shadow-[0_6px_20px_rgba(22,24,29,0.25)]"
        >
          {toast}
        </div>
      )}
    </div>
  );
}
