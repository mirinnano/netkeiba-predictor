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
import { DEFAULT_WEIGHTS } from "./components/WeightsPanel";
import { postTime, recommendedKey } from "./lib/format";
import { JvDataPanel } from "./components/JvDataPanel";
import { TodayView } from "./components/TodayView";
import { WinCelebration } from "./components/WinCelebration";

type Tab = "today" | "race" | "browser" | "bets" | "history";

const TABS: { id: Tab; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "race", label: "出馬表" },
  { id: "browser", label: "開催一覧" },
  { id: "bets", label: "買い目" },
  { id: "history", label: "回収率" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("today");
  const [raceInput, setRaceInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [busyText, setBusyText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [prediction, setPrediction] = useState<RacePredictionResult | null>(null);
  const [weights, setWeights] = useState<ScoringWeights>(DEFAULT_WEIGHTS);
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

  // オッズを最後に取った時刻と、取り直し中か
  const [oddsAt, setOddsAt] = useState<Date | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [now, setNow] = useState(() => new Date());
  const [celebrate, setCelebrate] = useState<RacePredictionResult | null>(null);

  const [toastShown, setToastShown] = useState(false);
  const toastMsg = useRef<string | null>(null);
  const showToast = (msg: string) => {
    toastMsg.current = msg;
    setToast(msg);
    // 一度 false で描画してから true にして、入りの transition を走らせる
    setToastShown(false);
    requestAnimationFrame(() => requestAnimationFrame(() => setToastShown(true)));
    window.setTimeout(() => {
      if (toastMsg.current !== msg) return;
      setToastShown(false);
      window.setTimeout(() => setToast((cur) => (toastMsg.current === msg ? null : cur)), 220);
    }, 3500);
  };

  useEffect(() => {
    refreshDashboard();
    refreshJvStatus();
    loadCalendar(undefined, false);
    // DB の更新までの成績を netkeiba から補う（裏で静かに。補ったときだけ知らせる）
    invoke<number>("sync_recent_results")
      .then((n) => {
        if (n > 0) {
          showToast(`最近の成績を ${n} レース分補いました`);
          refreshJvStatus();
        }
      })
      .catch(() => {});
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
    setBusyText("出馬表とオッズを取得中");
    setError(null);
    try {
      const res = await invoke<RacePredictionResult>("analyze_race", { input: q, weights, budget, strategy });
      setPrediction(res);
      setMemo(res.memo || "");
      setOddsAt(new Date());
      setTab("race");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setBusyText("");
    }
  };

  /** オッズだけ取り直して、期待値とおすすめを作り直す。おすすめが変わったら知らせる */
  const refreshOdds = async (auto = false) => {
    if (!prediction || refreshing || busy) return;
    setRefreshing(true);
    try {
      const before = recommendedKey(prediction);
      const updated = await invoke<RacePredictionResult>("refresh_odds", {
        currentResult: prediction,
        budget,
        strategy,
        memo,
      });
      setPrediction(updated);
      setOddsAt(new Date());
      const after = recommendedKey(updated);
      if (before !== after) {
        showToast(after ? `おすすめが変わりました：単勝 ${after.replace(/,/g, "・")}` : "おすすめの単勝がなくなりました");
      } else if (!auto) {
        showToast("オッズを更新しました");
      }
    } catch (e) {
      if (!auto) setError(String(e));
    } finally {
      setRefreshing(false);
    }
  };

  // 発走 20 分前から、2 分ごとにオッズを自動で取り直す（出馬表を開いているときだけ）
  const post = prediction ? postTime(prediction.race_info.date_str, prediction.race_info.start_time) : null;
  const minutesToPost = post ? Math.round((post.getTime() - now.getTime()) / 60000) : null;
  const autoRefresh = !!prediction && !prediction.race_info.is_finished && minutesToPost !== null && minutesToPost <= 20 && minutesToPost >= -2;
  // 払戻速報: 発走 1 分後から 45 分後まで、20 秒ごとに結果ページを見に行く。出たらすぐ当てはめる
  const watchPayouts = !!prediction && !prediction.race_info.is_finished && minutesToPost !== null && minutesToPost <= -1 && minutesToPost >= -45;
  const checking = useRef(false);
  const latest = useRef(prediction);
  latest.current = prediction;
  useEffect(() => {
    if (!watchPayouts) return;
    const check = async () => {
      if (checking.current) return;
      checking.current = true;
      try {
        const current = latest.current;
        if (!current || current.race_info.is_finished) return;
        const done = await invoke<RacePredictionResult | null>("check_payouts", { currentResult: current });
        if (!done) return;
        const saved = current.saved_at ? await invoke<RacePredictionResult>("save_prediction_record", { result: done }).catch(() => done) : done;
        setPrediction(saved);
        if ((saved.total_return ?? 0) > 0) setCelebrate(saved);
        else showToast("払戻が出ました。今回は外れです");
      } catch {
        /* 通信できないときは次の回に任せる */
      } finally {
        checking.current = false;
      }
    };
    check();
    const id = window.setInterval(check, 20000);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchPayouts, prediction?.race_info.race_id]);

  useEffect(() => {
    const id = window.setInterval(() => setNow(new Date()), 30000);
    return () => window.clearInterval(id);
  }, []);
  useEffect(() => {
    if (!autoRefresh || tab !== "race" || !oddsAt) return;
    if (now.getTime() - oddsAt.getTime() >= 120000) refreshOdds(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [now, autoRefresh, tab]);

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
          : `${saved.race_info.race_name} を保存しました（集計外）`,
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

  const renderTab = (t: (typeof TABS)[number], bottom: boolean) => {
    const disabled = (t.id === "race" || t.id === "bets") && !prediction;
    const on = tab === t.id;
    return (
      <button
        key={t.id}
        type="button"
        disabled={disabled}
        aria-current={on ? "page" : undefined}
        onClick={() => {
          setTab(t.id);
          if (t.id === "history") refreshDashboard();
        }}
        className={
          bottom
            ? `flex min-h-14 flex-1 flex-col items-center justify-center gap-0.5 text-[12px] font-bold transition-colors disabled:opacity-35 ${
                on ? "text-shell" : "text-ink-muted"
              }`
            : `rounded-md px-3.5 py-2 text-[13px] font-bold transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
                on ? "bg-paper text-shell" : "text-shell-text hover:bg-shell-deep hover:text-white"
              }`
        }
      >
        <span>
          {t.label}
          {t.id === "bets" && prediction && (
            <span className="num ml-1 text-[14px] opacity-70">
              {prediction.recommendations.filter((r) => r.suggested_amount > 0).length}
            </span>
          )}
        </span>
      </button>
    );
  };

  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-30 bg-shell text-white">
        <div className="mx-auto flex h-14 max-w-[1600px] items-center gap-6 px-6 max-md:h-auto max-md:flex-wrap max-md:gap-x-3 max-md:gap-y-2 max-md:px-4 max-md:pb-2.5 max-md:pt-[max(10px,env(safe-area-inset-top))]">
          <span className="text-[17px] font-black tracking-wide">KeibaScope</span>
          <nav aria-label="画面" className="flex gap-1 max-md:hidden">
            {TABS.map((t) => renderTab(t, false))}
          </nav>
          <form
            className={`ml-auto flex w-[460px] max-w-[45%] items-center gap-2 max-md:ml-0 max-md:w-full max-md:max-w-none ${
              tab === "today" || tab === "browser" ? "" : "max-md:hidden"
            }`}
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
                className="num h-9 w-full rounded-md border border-shell-edge bg-shell-deep pl-8 pr-2 text-[15px] max-md:h-11 max-md:text-[16px] text-white placeholder:font-sans placeholder:text-[12px] placeholder:text-shell-text focus:border-white focus:outline-none"
              />
            </div>
            <button
              type="submit"
              disabled={busy || !raceInput.trim()}
              className="flex h-9 items-center gap-1.5 rounded-md bg-paper px-4 text-[13px] font-bold text-shell hover:bg-ground disabled:opacity-50 max-md:h-11"
            >
              {busy && <Loader2 className="h-4 w-4 animate-spin" />}
              予想する
            </button>
          </form>
        </div>
        {busy && busyText && (
          <div className="h-0.5 overflow-hidden bg-shell-deep" role="progressbar" aria-label={busyText}>
            <div className="h-full w-1/3 progress-bar bg-waku-5" />
          </div>
        )}
      </header>

      <main className="mx-auto max-w-[1600px] px-6 py-5 max-md:px-4 max-md:pb-[calc(88px+env(safe-area-inset-bottom))] max-md:pt-4">
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
            budget={budget}
            strategy={strategy}
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
            onRefreshOdds={() => refreshOdds(false)}
            refreshing={refreshing}
            oddsAt={oddsAt}
            minutesToPost={minutesToPost}
            autoRefresh={autoRefresh}
            busy={busy}
            memo={memo}
            onMemo={setMemo}
          />
        )}
        {tab === "bets" && prediction && <BetsView result={prediction} />}
        {tab === "today" && <TodayView onPickRace={(id) => analyze(id)} onOpenBrowser={() => setTab("browser")} />}
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
      <nav
        aria-label="画面"
        className="fixed inset-x-0 bottom-0 z-30 flex border-t border-line bg-paper pb-[env(safe-area-inset-bottom)] md:hidden"
      >
        <span
          aria-hidden
          className="tab-indicator pointer-events-none absolute left-0 top-0 flex justify-center"
          style={{
            width: `${100 / TABS.length}%`,
            transform: `translateX(${Math.max(0, TABS.findIndex((t) => t.id === tab)) * 100}%)`,
          }}
        >
          <span className="h-0.5 w-8 rounded-full bg-shell" />
        </span>
        {TABS.map((t) => renderTab(t, true))}
      </nav>

      {celebrate && <WinCelebration result={celebrate} onClose={() => setCelebrate(null)} />}

      {toast && (
        <div
          role="status"
          data-shown={toastShown}
          className="toast fixed bottom-5 right-5 z-40 max-md:inset-x-4 max-md:bottom-[calc(68px+env(safe-area-inset-bottom))] max-md:text-center rounded-md bg-ink px-4 py-2.5 text-[13px] text-white shadow-[0_6px_20px_rgba(22,24,29,0.25)]"
        >
          {toast}
        </div>
      )}
    </div>
  );
}
