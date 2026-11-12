import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Award,
  BarChart3,
  BookmarkCheck,
  Calendar,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Coins,
  Compass,
  Database,
  ExternalLink,
  Flame,
  Gauge,
  History,
  Info,
  Loader2,
  RefreshCw,
  Save,
  Search,
  Sliders,
  Sparkles,
  Target,
  Trash2,
  TrendingUp,
  Trophy,
} from "lucide-react";
import {
  HorsePrediction,
  KaisaiCalendarResponse,
  RacePredictionResult,
  RoiDashboardStats,
  ScoringWeights,
  VenueRaces,
} from "./types";

type ActiveTab = "predict" | "browser" | "betting" | "history";

const WAKU_STYLES: Record<number, string> = {
  1: "bg-white text-slate-900 border border-slate-300",
  2: "bg-slate-900 text-white border border-slate-600",
  3: "bg-red-600 text-white",
  4: "bg-blue-600 text-white",
  5: "bg-yellow-400 text-slate-950 font-bold",
  6: "bg-emerald-600 text-white",
  7: "bg-orange-500 text-white",
  8: "bg-pink-500 text-white",
};

const MARK_STYLES: Record<string, string> = {
  "◎": "bg-rose-500/20 text-rose-400 border-rose-500/50 font-black",
  "○": "bg-sky-500/20 text-sky-400 border-sky-500/50 font-bold",
  "▲": "bg-amber-500/20 text-amber-400 border-amber-500/50 font-bold",
  "☆": "bg-purple-500/20 text-purple-300 border-purple-500/50 font-bold",
  "△": "bg-slate-700/60 text-slate-300 border-slate-600 font-semibold",
  "消": "bg-slate-900 text-slate-600 border-slate-800",
};

const QUICK_SAMPLE_RACES = [
  { label: "地方・門別12R (本日開催)", id: "202630111212" },
  { label: "地方・門別1R (確定済)", id: "202630111201" },
  { label: "2026 エリザベス女王杯 (GI・今週)", id: "202608050411" },
  { label: "2026 武蔵野S (GIII・今週)", id: "202605050311" },
  { label: "2026 アルゼンチン共和国杯 (GII)", id: "202605050211" },
  { label: "2023 ジャパンC (GI)", id: "202305050812" },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<ActiveTab>("predict");
  const [raceInput, setRaceInput] = useState<string>("202305050812");
  const [loading, setLoading] = useState<boolean>(false);
  const [loadingMessage, setLoadingMessage] = useState<string>("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  // Prediction state
  const [prediction, setPrediction] = useState<RacePredictionResult | null>(null);
  const [weights, setWeights] = useState<ScoringWeights>({
    ability: 30,
    speed: 25,
    aptitude: 20,
    jockey_frame: 15,
    pace: 10,
  });
  const [budget, setBudget] = useState<number>(3000);
  const [strategy, setStrategy] = useState<string>("balance");
  const [memo, setMemo] = useState<string>("");
  const [sortBy, setSortBy] = useState<"rank" | "umaban">("rank");
  const [expandedUmaban, setExpandedUmaban] = useState<number | null>(null);
  const [showWeightsPanel, setShowWeightsPanel] = useState<boolean>(false);

  // Race Browser state
  const [isNarBrowser, setIsNarBrowser] = useState<boolean>(false);
  const [calendar, setCalendar] = useState<KaisaiCalendarResponse | null>(null);
  const [selectedDate, setSelectedDate] = useState<string>("");
  const [customDateInput, setCustomDateInput] = useState<string>("20261108");
  const [venues, setVenues] = useState<VenueRaces[]>([]);
  const [browserLoading, setBrowserLoading] = useState<boolean>(false);

  // SQLite Dashboard state
  const [dashboard, setDashboard] = useState<RoiDashboardStats | null>(null);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    setTimeout(() => {
      setToastMsg((prev) => (prev === msg ? null : prev));
    }, 3500);
  };

  // Initial load: fetch dashboard stats, initial race, and race calendar
  useEffect(() => {
    refreshDashboard();
    loadCalendar(undefined, false);
    handleAnalyzeRace("202305050812");
  }, []);

  const refreshDashboard = async () => {
    try {
      const stats = await invoke<RoiDashboardStats>("get_roi_dashboard");
      setDashboard(stats);
    } catch (e) {
      console.error("Dashboard load error:", e);
    }
  };

  const loadCalendar = async (targetDate?: string, narMode?: boolean) => {
    const useNar = narMode ?? isNarBrowser;
    setBrowserLoading(true);
    try {
      const cal = await invoke<KaisaiCalendarResponse>("get_kaisai_calendar", {
        targetDate: targetDate || null,
        isNar: useNar,
      });
      setCalendar(cal);
      const dateToLoad = targetDate || cal.current_target;
      setSelectedDate(dateToLoad);
      setCustomDateInput(dateToLoad);
      const vList = await invoke<VenueRaces[]>("get_race_list_by_date", {
        kaisaiDate: dateToLoad,
        isNar: useNar,
      });
      setVenues(vList);
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setBrowserLoading(false);
    }
  };

  const handleSelectDate = async (dateStr: string) => {
    setSelectedDate(dateStr);
    setCustomDateInput(dateStr);
    setBrowserLoading(true);
    setErrorMsg(null);
    try {
      const vList = await invoke<VenueRaces[]>("get_race_list_by_date", {
        kaisaiDate: dateStr,
        isNar: isNarBrowser,
      });
      setVenues(vList);
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setBrowserLoading(false);
    }
  };

  const handleAnalyzeRace = async (targetInput?: string) => {
    const query = (targetInput ?? raceInput).trim();
    if (!query) return;
    setRaceInput(query);
    setLoading(true);
    setLoadingMessage("netkeibaから出馬表・過去5走・オッズ・結果データを取得＆指数計算中...");
    setErrorMsg(null);
    try {
      const res = await invoke<RacePredictionResult>("analyze_race", {
        input: query,
        weights,
        budget,
        strategy,
      });
      setPrediction(res);
      setMemo(res.memo || "");
      setExpandedUmaban(null);
      setActiveTab("predict");
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setLoading(false);
      setLoadingMessage("");
    }
  };

  const handleRecalculate = async (
    newWeights?: ScoringWeights,
    newBudget?: number,
    newStrategy?: string,
  ) => {
    if (!prediction) return;
    const w = newWeights ?? weights;
    const b = newBudget ?? budget;
    const s = newStrategy ?? strategy;
    try {
      const updated = await invoke<RacePredictionResult>("recalculate_prediction", {
        currentResult: prediction,
        weights: w,
        budget: b,
        strategy: s,
        memo,
      });
      setPrediction(updated);
      showToast("スコアリング重み・買い目配分を再計算しました");
    } catch (e) {
      setErrorMsg(String(e));
    }
  };

  const applyWeightPreset = (presetName: string, w: ScoringWeights) => {
    setWeights(w);
    handleRecalculate(w, budget, strategy);
    showToast(`プリセット「${presetName}」を適用しました`);
  };

  const handleSaveToDb = async () => {
    if (!prediction) return;
    try {
      const payload: RacePredictionResult = {
        ...prediction,
        memo,
      };
      const saved = await invoke<RacePredictionResult>("save_prediction_record", {
        result: payload,
      });
      setPrediction(saved);
      await refreshDashboard();
      showToast(`「${saved.race_info.race_name}」の予想と買い目をSQLiteに保存しました`);
    } catch (e) {
      setErrorMsg(String(e));
    }
  };

  const handleVerifySavedRace = async (raceId: string) => {
    setLoading(true);
    setLoadingMessage(`レースID ${raceId} の確定結果・払戻データをnetkeibaから取得照合中...`);
    setErrorMsg(null);
    try {
      const updated = await invoke<RacePredictionResult>("verify_saved_race_result", {
        raceId,
      });
      setPrediction(updated);
      await refreshDashboard();
      showToast(
        updated.race_info.is_finished
          ? `確定結果を反映しました（払戻: ${(updated.total_return ?? 0).toLocaleString()}円）`
          : "まだ確定結果が公開されていません",
      );
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setLoading(false);
      setLoadingMessage("");
    }
  };

  const handleLoadSaved = async (raceId: string) => {
    try {
      const loaded = await invoke<RacePredictionResult>("load_prediction_record", {
        raceId,
      });
      setPrediction(loaded);
      setWeights(loaded.weights);
      setBudget(loaded.budget);
      setStrategy(loaded.strategy);
      setMemo(loaded.memo);
      setRaceInput(loaded.race_info.race_id);
      setActiveTab("predict");
      showToast(`保存済みデータ「${loaded.race_info.race_name}」を読み込みました`);
    } catch (e) {
      setErrorMsg(String(e));
    }
  };

  const handleDeleteSaved = async (raceId: string) => {
    try {
      const stats = await invoke<RoiDashboardStats>("delete_prediction_record", {
        raceId,
      });
      setDashboard(stats);
      showToast("保存済みレコードを削除しました");
    } catch (e) {
      setErrorMsg(String(e));
    }
  };

  const sortedPredictions: HorsePrediction[] = prediction
    ? [...prediction.predictions].sort((a, b) => {
        if (sortBy === "umaban") {
          return a.entry.umaban - b.entry.umaban;
        }
        return a.rank_order - b.rank_order || a.entry.umaban - b.entry.umaban;
      })
    : [];

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col">
      {/* Top Header & URL/ID Search Bar */}
      <header className="sticky top-0 z-30 bg-slate-900/95 backdrop-blur border-b border-slate-800 px-5 py-3 shadow-lg">
        <div className="max-w-[1600px] mx-auto flex flex-col lg:flex-row items-stretch lg:items-center justify-between gap-3">
          {/* Brand Title */}
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-emerald-500 to-teal-700 flex items-center justify-center shadow-md shadow-emerald-950/60">
              <Trophy className="w-5 h-5 text-white" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-lg font-extrabold tracking-tight text-white">
                  KeibaScope
                </h1>
                <span className="text-[11px] font-semibold uppercase tracking-wider px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400 border border-emerald-500/30">
                  netkeiba Scraper & AI Index
                </span>
              </div>
              <p className="text-xs text-slate-400">
                出馬表・過去5走・リアルタイムオッズ解析＆期待値・回収率シミュレーター
              </p>
            </div>
          </div>

          {/* Search Input Bar */}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              handleAnalyzeRace();
            }}
            className="flex-1 max-w-2xl flex items-center gap-2"
          >
            <div className="relative flex-1">
              <Search className="w-4 h-4 text-slate-400 absolute left-3.5 top-1/2 -translate-y-1/2" />
              <input
                type="text"
                value={raceInput}
                onChange={(e) => setRaceInput(e.target.value)}
                placeholder="netkeiba出馬表URL または 12桁レースID (例: 202305050812)"
                className="w-full pl-10 pr-4 py-2 text-sm bg-slate-950 border border-slate-700 rounded-lg text-slate-100 placeholder-slate-500 focus:outline-none focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500"
              />
            </div>
            <button
              type="submit"
              disabled={loading}
              className="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-sm font-bold rounded-lg shadow flex items-center gap-2 transition cursor-pointer shrink-0"
            >
              {loading ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                <Sparkles className="w-4 h-4" />
              )}
              スクレイピング予想
            </button>
          </form>

          {/* Navigation Tabs */}
          <nav className="flex items-center gap-1.5 bg-slate-950 p-1 rounded-xl border border-slate-800">
            <button
              onClick={() => setActiveTab("predict")}
              className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition cursor-pointer ${
                activeTab === "predict"
                  ? "bg-emerald-600 text-white shadow"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-900"
              }`}
            >
              <Gauge className="w-3.5 h-3.5" />
              出馬表・指数予想
            </button>
            <button
              onClick={() => setActiveTab("browser")}
              className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition cursor-pointer ${
                activeTab === "browser"
                  ? "bg-emerald-600 text-white shadow"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-900"
              }`}
            >
              <Calendar className="w-3.5 h-3.5" />
              開催レース一覧
            </button>
            <button
              onClick={() => setActiveTab("betting")}
              className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition cursor-pointer ${
                activeTab === "betting"
                  ? "bg-emerald-600 text-white shadow"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-900"
              }`}
            >
              <Coins className="w-3.5 h-3.5" />
              買い目・期待値
              {prediction && (
                <span className="px-1.5 py-0.2 bg-slate-900/60 rounded text-[10px]">
                  {prediction.recommendations.length}点
                </span>
              )}
            </button>
            <button
              onClick={() => {
                refreshDashboard();
                setActiveTab("history");
              }}
              className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition cursor-pointer ${
                activeTab === "history"
                  ? "bg-emerald-600 text-white shadow"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-900"
              }`}
            >
              <Database className="w-3.5 h-3.5" />
              保存履歴・回収率
              {dashboard && dashboard.total_saved_races > 0 && (
                <span className="px-1.5 py-0.2 bg-slate-900/60 rounded text-[10px]">
                  {dashboard.total_saved_races}
                </span>
              )}
            </button>
          </nav>
        </div>

        {/* Quick Sample Pills */}
        <div className="max-w-[1600px] mx-auto mt-2 pt-2 border-t border-slate-800/70 flex items-center justify-between gap-2 flex-wrap text-xs">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="text-slate-500 font-medium">クイック検証レース:</span>
            {QUICK_SAMPLE_RACES.map((s) => (
              <button
                key={s.id}
                onClick={() => handleAnalyzeRace(s.id)}
                className={`px-2.5 py-0.5 rounded-md border text-xs transition cursor-pointer ${
                  prediction?.race_info.race_id === s.id
                    ? "bg-emerald-500/20 border-emerald-500/50 text-emerald-300 font-semibold"
                    : "bg-slate-950 border-slate-800 text-slate-400 hover:text-slate-200 hover:border-slate-700"
                }`}
              >
                {s.label}
              </button>
            ))}
            <button
              onClick={() => {
                setIsNarBrowser(false);
                loadCalendar(undefined, false);
                setActiveTab("browser");
              }}
              className="px-2.5 py-0.5 rounded-md bg-sky-500/15 border border-sky-500/30 text-sky-300 hover:bg-sky-500/25 transition cursor-pointer flex items-center gap-1"
            >
              <Compass className="w-3 h-3" />
              中央競馬(JRA)一覧
            </button>
            <button
              onClick={() => {
                setIsNarBrowser(true);
                loadCalendar(undefined, true);
                setActiveTab("browser");
              }}
              className="px-2.5 py-0.5 rounded-md bg-amber-500/15 border border-amber-500/30 text-amber-300 hover:bg-amber-500/25 transition cursor-pointer flex items-center gap-1"
            >
              <Compass className="w-3 h-3" />
              地方競馬(NAR)一覧
            </button>
          </div>

          {dashboard && (
            <div className="flex items-center gap-4 text-xs text-slate-400">
              <span>
                保存数: <strong className="text-slate-200">{dashboard.total_saved_races}</strong>
              </span>
              <span>
                累計投資:{" "}
                <strong className="text-slate-200">
                  {dashboard.total_investment.toLocaleString()}円
                </strong>
              </span>
              <span>
                累計払戻:{" "}
                <strong className="text-emerald-400">
                  {dashboard.total_return.toLocaleString()}円
                </strong>
              </span>
              <span>
                通算回収率:{" "}
                <strong
                  className={
                    dashboard.overall_roi >= 100
                      ? "text-emerald-400 font-bold"
                      : "text-amber-400 font-bold"
                  }
                >
                  {dashboard.overall_roi.toFixed(1)}%
                </strong>
              </span>
            </div>
          )}
        </div>
      </header>

      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-5 right-5 z-50 bg-emerald-900/95 border border-emerald-500/50 text-emerald-100 px-4 py-2.5 rounded-xl shadow-2xl flex items-center gap-2 text-sm animate-fade-in">
          <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
          <span>{toastMsg}</span>
        </div>
      )}

      {/* Main Content Container */}
      <main className="flex-1 max-w-[1600px] w-full mx-auto px-5 py-5 space-y-5">
        {/* Error Alert */}
        {errorMsg && (
          <div className="bg-red-950/70 border border-red-500/50 text-red-200 px-4 py-3 rounded-xl flex items-center justify-between text-sm">
            <div className="flex items-center gap-2">
              <Info className="w-4 h-4 text-red-400 shrink-0" />
              <span>{errorMsg}</span>
            </div>
            <button
              onClick={() => setErrorMsg(null)}
              className="text-xs text-red-300 hover:text-white underline ml-4"
            >
              閉じる
            </button>
          </div>
        )}

        {/* Loading Overlay Banner */}
        {loading && (
          <div className="bg-slate-900/90 border border-emerald-500/40 rounded-xl p-6 flex items-center justify-center gap-3 text-emerald-300 shadow-lg">
            <Loader2 className="w-6 h-6 animate-spin text-emerald-400" />
            <span className="text-sm font-semibold">{loadingMessage}</span>
          </div>
        )}

        {/* ================= TAB 2: RACE BROWSER ================= */}
        {activeTab === "browser" && (
          <div className="space-y-4">
            <div className="bg-slate-900 border border-slate-800 rounded-2xl p-4">
              <div className="flex flex-wrap items-center justify-between gap-4 mb-4">
                <div className="flex flex-wrap items-center gap-4">
                  <div>
                    <h2 className="text-base font-bold text-white flex items-center gap-2">
                      <Calendar className="w-5 h-5 text-emerald-400" />
                      netkeiba 開催日・競馬場別レース一覧ブラウザ
                    </h2>
                    <p className="text-xs text-slate-400 mt-0.5">
                      中央競馬（JRA）と地方競馬（NAR）を切り替えて各場の1R〜12Rをワンクリック予想できます。
                    </p>
                  </div>

                  {/* JRA / NAR Toggle */}
                  <div className="flex items-center gap-1.5 bg-slate-950 p-1 rounded-xl border border-slate-800">
                    <button
                      type="button"
                      onClick={() => {
                        setIsNarBrowser(false);
                        loadCalendar(undefined, false);
                      }}
                      className={`px-3.5 py-1.5 rounded-lg text-xs font-extrabold transition cursor-pointer ${
                        !isNarBrowser
                          ? "bg-emerald-600 text-white shadow"
                          : "text-slate-400 hover:text-slate-200"
                      }`}
                    >
                      中央競馬 (JRA)
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setIsNarBrowser(true);
                        loadCalendar(undefined, true);
                      }}
                      className={`px-3.5 py-1.5 rounded-lg text-xs font-extrabold transition cursor-pointer ${
                        isNarBrowser
                          ? "bg-amber-500 text-slate-950 shadow"
                          : "text-slate-400 hover:text-slate-200"
                      }`}
                    >
                      地方競馬 (NAR)
                    </button>
                  </div>
                </div>

                {/* Custom Date Input */}
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (/^\d{8}$/.test(customDateInput.trim())) {
                      loadCalendar(customDateInput.trim(), isNarBrowser);
                    } else {
                      setErrorMsg("開催日は YYYYMMDD 形式（例: 20261112 または 20261108）で入力してください");
                    }
                  }}
                  className="flex items-center gap-2"
                >
                  <input
                    type="text"
                    value={customDateInput}
                    onChange={(e) => setCustomDateInput(e.target.value)}
                    placeholder="YYYYMMDD (例: 20261112)"
                    className="px-3 py-1.5 text-xs bg-slate-950 border border-slate-700 rounded-lg text-slate-200 w-40"
                  />
                  <button
                    type="submit"
                    className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-xs font-semibold rounded-lg text-slate-200 cursor-pointer"
                  >
                    日付移動
                  </button>
                  <button
                    type="button"
                    onClick={() => loadCalendar(undefined, isNarBrowser)}
                    className="px-3 py-1.5 bg-emerald-600/20 hover:bg-emerald-600/30 border border-emerald-500/40 text-xs font-semibold rounded-lg text-emerald-300 cursor-pointer"
                  >
                    最新開催へ
                  </button>
                </form>
              </div>

              {/* Date Tabs Bar */}
              {calendar && (
                <div className="flex items-center gap-2 overflow-x-auto pb-2">
                  {calendar.prev_date && (
                    <button
                      onClick={() => loadCalendar(calendar.prev_date!)}
                      className="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-800 hover:border-slate-700 text-xs text-slate-300 flex items-center gap-1 shrink-0 cursor-pointer"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      前開催
                    </button>
                  )}
                  {calendar.dates.map((d) => (
                    <button
                      key={d.date}
                      onClick={() => handleSelectDate(d.date)}
                      className={`px-3.5 py-2 rounded-xl text-xs font-bold border transition shrink-0 cursor-pointer ${
                        selectedDate === d.date
                          ? "bg-emerald-600 border-emerald-400 text-white shadow"
                          : d.is_sunday
                          ? "bg-slate-950 border-slate-800 text-rose-400 hover:border-rose-500/40"
                          : d.is_saturday
                          ? "bg-slate-950 border-slate-800 text-sky-400 hover:border-sky-500/40"
                          : "bg-slate-950 border-slate-800 text-slate-300 hover:border-slate-700"
                      }`}
                    >
                      <div>{d.label}</div>
                      <div className="text-[10px] opacity-75 font-normal">{d.date}</div>
                    </button>
                  ))}
                  {calendar.next_date && (
                    <button
                      onClick={() => loadCalendar(calendar.next_date!)}
                      className="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-800 hover:border-slate-700 text-xs text-slate-300 flex items-center gap-1 shrink-0 cursor-pointer"
                    >
                      次開催
                      <ChevronRight className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              )}
            </div>

            {browserLoading ? (
              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-12 flex flex-col items-center justify-center gap-3 text-slate-400">
                <Loader2 className="w-7 h-7 animate-spin text-emerald-400" />
                <span className="text-sm">開催レース一覧を取得中...</span>
              </div>
            ) : venues.length === 0 ? (
              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-10 text-center text-slate-400 text-sm">
                指定された日付（{selectedDate}）の{isNarBrowser ? "地方競馬 (NAR)" : "中央競馬 (JRA)"}開催データが見つかりませんでした。上部の日付タブまたはYYYYMMDD入力（例: 20231126, 20261112）をお試しください。
              </div>
            ) : (
              <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                {venues.map((v) => (
                  <div
                    key={v.venue_name}
                    className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden flex flex-col"
                  >
                    <div className="bg-slate-800/70 px-4 py-3 border-b border-slate-700/70 flex items-center justify-between">
                      <div>
                        <h3 className="font-extrabold text-sm text-white">{v.venue_name}</h3>
                        <div className="flex items-center gap-2 text-[11px] text-slate-400 mt-0.5">
                          <span>天気: {v.weather}</span>
                          {v.turf_cond && <span>{v.turf_cond}</span>}
                          {v.dirt_cond && <span>{v.dirt_cond}</span>}
                        </div>
                      </div>
                      <span className="px-2.5 py-1 rounded-lg bg-emerald-500/15 text-emerald-300 font-bold text-xs">
                        全{v.races.length}R
                      </span>
                    </div>

                    <div className="divide-y divide-slate-800/80">
                      {v.races.map((r) => (
                        <div
                          key={r.race_id}
                          onClick={() => handleAnalyzeRace(r.race_id)}
                          className="px-4 py-2.5 hover:bg-slate-800/60 transition flex items-center justify-between gap-2 cursor-pointer group"
                        >
                          <div className="flex items-center gap-2.5 min-w-0">
                            <span className="w-8 h-7 rounded-md bg-slate-950 border border-slate-700 text-xs font-extrabold flex items-center justify-center text-slate-200 shrink-0 group-hover:border-emerald-500/60 group-hover:text-emerald-300">
                              {r.race_num}
                            </span>
                            <div className="min-w-0">
                              <div className="flex items-center gap-1.5">
                                <span className="text-xs font-bold text-slate-100 truncate group-hover:text-emerald-300">
                                  {r.title}
                                </span>
                                {r.grade && (
                                  <span
                                    className={`px-1.5 py-0.2 rounded text-[10px] font-black shrink-0 ${
                                      r.grade.includes("GI") && !r.grade.includes("II")
                                        ? "bg-amber-500 text-slate-950"
                                        : r.grade.includes("GII") && !r.grade.includes("III")
                                        ? "bg-rose-500 text-white"
                                        : r.grade.includes("GIII")
                                        ? "bg-emerald-500 text-slate-950"
                                        : "bg-slate-700 text-slate-200"
                                    }`}
                                  >
                                    {r.grade}
                                  </span>
                                )}
                              </div>
                              <div className="flex items-center gap-2 text-[11px] text-slate-400">
                                <span>{r.start_time}発走</span>
                                <span
                                  className={
                                    r.surface === "芝"
                                      ? "text-emerald-400 font-semibold"
                                      : r.surface === "ダ"
                                      ? "text-amber-400 font-semibold"
                                      : "text-sky-400 font-semibold"
                                  }
                                >
                                  {r.surface}
                                  {r.distance}m
                                </span>
                                {r.head_count > 0 && <span>{r.head_count}頭</span>}
                              </div>
                            </div>
                          </div>

                          <span className="text-[11px] font-bold text-emerald-400 opacity-0 group-hover:opacity-100 transition shrink-0 flex items-center gap-1">
                            予想する
                            <ChevronRight className="w-3.5 h-3.5" />
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* ================= TAB 1: PREDICTION STUDIO ================= */}
        {activeTab === "predict" && prediction && (
          <div className="space-y-4">
            {/* Race Info Banner */}
            <div className="bg-gradient-to-r from-slate-900 via-slate-900 to-emerald-950/40 border border-slate-800 rounded-2xl p-5 shadow-lg">
              <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
                <div className="space-y-1.5">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="px-2.5 py-0.5 rounded-md bg-slate-800 text-slate-300 text-xs font-bold">
                      {prediction.race_info.date_str}
                    </span>
                    <span className="px-2.5 py-0.5 rounded-md bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-xs font-extrabold">
                      {prediction.race_info.venue} {prediction.race_info.race_num}
                    </span>
                    {prediction.race_info.grade && (
                      <span className="px-2.5 py-0.5 rounded-md bg-amber-500 text-slate-950 text-xs font-black">
                        {prediction.race_info.grade}
                      </span>
                    )}
                    <span
                      className={`px-2.5 py-0.5 rounded-md text-xs font-bold ${
                        prediction.race_info.is_finished
                          ? "bg-sky-500/20 border border-sky-500/40 text-sky-300"
                          : "bg-amber-500/20 border border-amber-500/40 text-amber-300"
                      }`}
                    >
                      {prediction.race_info.is_finished ? "結果確定済 (自動検証対応)" : "開催前・予想モード"}
                    </span>
                    <span className="text-xs text-slate-500 font-mono">
                      ID: {prediction.race_info.race_id}
                    </span>
                  </div>

                  <div className="flex items-baseline gap-3 flex-wrap">
                    <h2 className="text-2xl font-black text-white tracking-tight">
                      {prediction.race_info.race_name}
                    </h2>
                    <div className="flex items-center gap-3 text-xs text-slate-300 font-semibold">
                      <span
                        className={`px-2 py-0.5 rounded ${
                          prediction.race_info.surface === "芝"
                            ? "bg-emerald-900/60 text-emerald-300 border border-emerald-700/50"
                            : "bg-amber-900/60 text-amber-300 border border-amber-700/50"
                        }`}
                      >
                        {prediction.race_info.surface}
                        {prediction.race_info.distance}m ({prediction.race_info.direction})
                      </span>
                      <span>発走 {prediction.race_info.start_time}</span>
                      <span>天候: {prediction.race_info.weather}</span>
                      <span>馬場: {prediction.race_info.track_cond}</span>
                      <span>{prediction.race_info.head_count}頭立</span>
                    </div>
                  </div>
                </div>

                {/* Action Buttons */}
                <div className="flex items-center gap-2 flex-wrap">
                  <button
                    onClick={() => setShowWeightsPanel(!showWeightsPanel)}
                    className={`px-3.5 py-2 rounded-xl text-xs font-bold border flex items-center gap-1.5 transition cursor-pointer ${
                      showWeightsPanel
                        ? "bg-emerald-600/25 border-emerald-500 text-emerald-300"
                        : "bg-slate-800 border-slate-700 text-slate-200 hover:bg-slate-700"
                    }`}
                  >
                    <Sliders className="w-4 h-4" />
                    指数ウェイト調整
                  </button>

                  <button
                    onClick={() => handleVerifySavedRace(prediction.race_info.race_id)}
                    className="px-3.5 py-2 rounded-xl text-xs font-bold bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-200 flex items-center gap-1.5 transition cursor-pointer"
                    title="最新のオッズ・確定結果を再取得して照合します"
                  >
                    <RefreshCw className="w-4 h-4" />
                    オッズ・結果更新
                  </button>

                  <button
                    onClick={handleSaveToDb}
                    className="px-4 py-2 rounded-xl text-xs font-bold bg-emerald-600 hover:bg-emerald-500 text-white shadow flex items-center gap-1.5 transition cursor-pointer"
                  >
                    <Save className="w-4 h-4" />
                    SQLiteに予想を保存
                  </button>
                </div>
              </div>

              {/* Pace Analysis & Top Summary Strip */}
              <div className="mt-4 pt-4 border-t border-slate-800/80 grid grid-cols-1 lg:grid-cols-3 gap-4">
                <div className="lg:col-span-2 bg-slate-950/70 border border-slate-800/90 rounded-xl p-3.5 flex items-start gap-3">
                  <div className="p-2 rounded-lg bg-amber-500/15 text-amber-400 shrink-0 mt-0.5">
                    <Flame className="w-5 h-5" />
                  </div>
                  <div className="space-y-1 text-xs">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-bold text-slate-300">AI展開・ペース診断:</span>
                      <span className="px-2 py-0.5 rounded bg-amber-500/20 text-amber-300 font-bold">
                        {prediction.pace_analysis.predicted_pace}
                      </span>
                      <span className="px-2 py-0.5 rounded bg-emerald-500/20 text-emerald-300 font-bold">
                        {prediction.pace_analysis.favored_style}
                      </span>
                      {prediction.pace_analysis.front_runners.length > 0 && (
                        <span className="text-slate-400">
                          逃げ候補: {prediction.pace_analysis.front_runners.join(", ")}
                        </span>
                      )}
                    </div>
                    <p className="text-slate-300 leading-relaxed">
                      {prediction.pace_analysis.commentary}
                    </p>
                  </div>
                </div>

                {/* Quick Bet & Verification Summary Box */}
                <div className="bg-slate-950/70 border border-slate-800/90 rounded-xl p-3.5 flex items-center justify-between">
                  <div className="space-y-1">
                    <div className="text-xs text-slate-400">推奨買い目シミュレーション</div>
                    <div className="flex items-baseline gap-3">
                      <div>
                        <span className="text-[11px] text-slate-500">投資合計: </span>
                        <strong className="text-sm text-white">
                          {prediction.total_bet.toLocaleString()}円
                        </strong>
                      </div>
                      {prediction.race_info.is_finished && (
                        <div>
                          <span className="text-[11px] text-slate-500">確定払戻: </span>
                          <strong
                            className={`text-sm font-black ${
                              (prediction.total_return ?? 0) >= prediction.total_bet
                                ? "text-emerald-400"
                                : "text-amber-400"
                            }`}
                          >
                            {(prediction.total_return ?? 0).toLocaleString()}円 (
                            {prediction.roi_percent?.toFixed(1)}%)
                          </strong>
                        </div>
                      )}
                    </div>
                    <div className="text-[11px] text-slate-400">
                      本命◎:{" "}
                      <strong className="text-rose-400">
                        {prediction.predictions.find((p) => p.mark === "◎")?.entry.umaban}番{" "}
                        {prediction.predictions.find((p) => p.mark === "◎")?.entry.horse_name}
                      </strong>{" "}
                      / 特注☆:{" "}
                      <strong className="text-purple-300">
                        {prediction.predictions.find((p) => p.mark === "☆")?.entry.umaban}番{" "}
                        {prediction.predictions.find((p) => p.mark === "☆")?.entry.horse_name}
                      </strong>
                    </div>
                  </div>
                  <button
                    onClick={() => setActiveTab("betting")}
                    className="px-3 py-2 rounded-lg bg-emerald-600/20 hover:bg-emerald-600/30 border border-emerald-500/40 text-emerald-300 text-xs font-bold shrink-0 cursor-pointer"
                  >
                    買い目詳細 →
                  </button>
                </div>
              </div>

              {/* Interactive Scoring Weights Panel */}
              {showWeightsPanel && (
                <div className="mt-4 pt-4 border-t border-slate-800 bg-slate-950/90 rounded-xl p-4 space-y-4">
                  <div className="flex items-center justify-between flex-wrap gap-2">
                    <div className="flex items-center gap-2">
                      <Sliders className="w-4 h-4 text-emerald-400" />
                      <h3 className="text-xs font-bold text-white">
                        独自スコアリング指数 ウェイトカスタマイズ（変更すると即座に再計算されます）
                      </h3>
                    </div>
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="text-[11px] text-slate-400">プリセット:</span>
                      <button
                        onClick={() =>
                          applyWeightPreset("標準バランス", {
                            ability: 30,
                            speed: 25,
                            aptitude: 20,
                            jockey_frame: 15,
                            pace: 10,
                          })
                        }
                        className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 cursor-pointer"
                      >
                        標準バランス
                      </button>
                      <button
                        onClick={() =>
                          applyWeightPreset("時計・末脚重視", {
                            ability: 20,
                            speed: 45,
                            aptitude: 15,
                            jockey_frame: 10,
                            pace: 10,
                          })
                        }
                        className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 cursor-pointer"
                      >
                        時計・末脚重視
                      </button>
                      <button
                        onClick={() =>
                          applyWeightPreset("コース適性・騎手重視", {
                            ability: 20,
                            speed: 15,
                            aptitude: 35,
                            jockey_frame: 25,
                            pace: 5,
                          })
                        }
                        className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 cursor-pointer"
                      >
                        適性・騎手重視
                      </button>
                      <button
                        onClick={() =>
                          applyWeightPreset("展開・バイアス重視", {
                            ability: 20,
                            speed: 20,
                            aptitude: 15,
                            jockey_frame: 15,
                            pace: 30,
                          })
                        }
                        className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 cursor-pointer"
                      >
                        展開バイアス重視
                      </button>
                    </div>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
                    {[
                      { key: "ability", label: "基礎能力・過去着順", val: weights.ability },
                      { key: "speed", label: "時計・上がり3F指数", val: weights.speed },
                      { key: "aptitude", label: "距離・コース・馬場適性", val: weights.aptitude },
                      { key: "jockey_frame", label: "騎手・枠順・馬体重", val: weights.jockey_frame },
                      { key: "pace", label: "展開・脚質バイアス", val: weights.pace },
                    ].map((item) => (
                      <div
                        key={item.key}
                        className="bg-slate-900 border border-slate-800 rounded-lg p-3 space-y-1.5"
                      >
                        <div className="flex items-center justify-between text-xs">
                          <span className="text-slate-300 font-semibold">{item.label}</span>
                          <span className="text-emerald-400 font-bold">{item.val}</span>
                        </div>
                        <input
                          type="range"
                          min={0}
                          max={60}
                          step={5}
                          value={item.val}
                          onChange={(e) => {
                            const updated = {
                              ...weights,
                              [item.key]: Number(e.target.value),
                            };
                            setWeights(updated);
                            handleRecalculate(updated, budget, strategy);
                          }}
                          className="w-full accent-emerald-500 cursor-pointer"
                        />
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Table Controls Bar */}
            <div className="flex items-center justify-between flex-wrap gap-3">
              <div className="flex items-center gap-2">
                <span className="text-xs text-slate-400 font-medium">並び替え:</span>
                <button
                  onClick={() => setSortBy("rank")}
                  className={`px-3 py-1 rounded-lg text-xs font-bold transition cursor-pointer ${
                    sortBy === "rank"
                      ? "bg-emerald-600 text-white"
                      : "bg-slate-900 text-slate-400 border border-slate-800 hover:text-slate-200"
                  }`}
                >
                  AI指数・予想印順
                </button>
                <button
                  onClick={() => setSortBy("umaban")}
                  className={`px-3 py-1 rounded-lg text-xs font-bold transition cursor-pointer ${
                    sortBy === "umaban"
                      ? "bg-emerald-600 text-white"
                      : "bg-slate-900 text-slate-400 border border-slate-800 hover:text-slate-200"
                  }`}
                >
                  枠・馬番順
                </button>
                <span className="text-xs text-slate-500 ml-2">
                  ※各行をクリックすると過去5走の詳細ラップ・通過順・着差を展開表示します
                </span>
              </div>

              <div className="flex items-center gap-2">
                <input
                  type="text"
                  value={memo}
                  onChange={(e) => setMemo(e.target.value)}
                  placeholder="レースメモ・回顧ノートを入力..."
                  className="px-3 py-1 text-xs bg-slate-900 border border-slate-800 rounded-lg text-slate-200 w-64 focus:outline-none focus:border-emerald-500"
                />
              </div>
            </div>

            {/* Main Predictions Table */}
            <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden shadow-xl">
              <div className="overflow-x-auto">
                <table className="w-full text-left border-collapse">
                  <thead>
                    <tr className="bg-slate-950/90 border-b border-slate-800 text-[11px] text-slate-400 uppercase tracking-wider">
                      <th className="py-3 px-3 text-center w-12">印</th>
                      <th className="py-3 px-2 text-center w-10">枠</th>
                      <th className="py-3 px-2 text-center w-12">馬番</th>
                      <th className="py-3 px-3 min-w-[200px]">馬名 / 血統 / 脚質</th>
                      <th className="py-3 px-3 min-w-[125px]">騎手 / 斤量 / 馬体重</th>
                      <th className="py-3 px-3 w-40">AI総合偏差値</th>
                      <th className="py-3 px-3 min-w-[210px]">5項目スコア (能力/時計/適性/騎手/展開)</th>
                      <th className="py-3 px-3 text-right w-28">勝率 / 複勝率</th>
                      <th className="py-3 px-3 text-right w-28">単勝オッズ</th>
                      <th className="py-3 px-3 text-center w-28">期待値 / 妙味</th>
                      <th className="py-3 px-3 min-w-[180px]">AI評価ポイント</th>
                      {prediction.race_info.is_finished && (
                        <th className="py-3 px-3 text-center w-24">確定着順</th>
                      )}
                      <th className="py-3 px-2 text-center w-10">過去走</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/70 text-xs">
                    {sortedPredictions.map((p) => {
                      const e = p.entry;
                      const isExpanded = expandedUmaban === e.umaban;
                      const wakuClass = WAKU_STYLES[e.waku] || WAKU_STYLES[1];
                      const markClass =
                        MARK_STYLES[p.mark] || "bg-slate-900 text-slate-600 border-slate-800";

                      return (
                        <React.Fragment key={e.umaban}>
                          <tr
                            onClick={() =>
                              setExpandedUmaban(isExpanded ? null : e.umaban)
                            }
                            className={`transition cursor-pointer ${
                              e.is_scratched
                                ? "opacity-40 bg-slate-950"
                                : p.mark === "◎"
                                ? "bg-rose-950/15 hover:bg-rose-950/25"
                                : p.mark === "○"
                                ? "bg-sky-950/15 hover:bg-sky-950/25"
                                : p.mark === "☆"
                                ? "bg-purple-950/15 hover:bg-purple-950/25"
                                : "hover:bg-slate-800/50"
                            }`}
                          >
                            {/* 印 */}
                            <td className="py-3 px-3 text-center">
                              <span
                                className={`inline-flex items-center justify-center w-7 h-7 rounded-lg border text-sm ${markClass}`}
                              >
                                {p.mark || "-"}
                              </span>
                            </td>

                            {/* 枠 */}
                            <td className="py-3 px-2 text-center">
                              <span
                                className={`inline-flex items-center justify-center w-6 h-6 rounded text-xs font-bold ${wakuClass}`}
                              >
                                {e.waku}
                              </span>
                            </td>

                            {/* 馬番 */}
                            <td className="py-3 px-2 text-center font-mono font-extrabold text-sm text-slate-200">
                              {e.umaban}
                            </td>

                            {/* 馬名 / 血統 / 脚質 */}
                            <td className="py-3 px-3">
                              <div className="flex items-center gap-1.5">
                                <span className="font-extrabold text-sm text-white">
                                  {e.horse_name}
                                </span>
                                <span className="px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 text-[10px]">
                                  {e.sex_age}
                                </span>
                                {e.running_style && (
                                  <span
                                    className={`px-1.5 py-0.2 rounded text-[10px] font-bold ${
                                      e.running_style.includes("逃")
                                        ? "bg-rose-500/20 text-rose-300"
                                        : e.running_style.includes("先")
                                        ? "bg-amber-500/20 text-amber-300"
                                        : e.running_style.includes("差")
                                        ? "bg-sky-500/20 text-sky-300"
                                        : "bg-purple-500/20 text-purple-300"
                                    }`}
                                  >
                                    脚質:{e.running_style}
                                  </span>
                                )}
                              </div>
                              <div className="text-[11px] text-slate-400 mt-0.5 truncate">
                                {e.sire && <span>父:{e.sire} </span>}
                                {e.broodmare_sire && <span>(母父:{e.broodmare_sire})</span>}
                              </div>
                            </td>

                            {/* 騎手 / 斤量 / 馬体重 */}
                            <td className="py-3 px-3">
                              <div className="font-bold text-slate-200">
                                {e.jockey_name}{" "}
                                <span className="text-slate-400 font-normal">
                                  ({e.carried_weight.toFixed(1)}kg)
                                </span>
                              </div>
                              <div className="text-[11px] text-slate-400 mt-0.5 flex items-center gap-2">
                                <span>{e.trainer_name}</span>
                                {e.body_weight && (
                                  <span className="font-mono text-slate-300">
                                    {e.body_weight}kg(
                                    {e.weight_diff !== null && e.weight_diff > 0
                                      ? `+${e.weight_diff}`
                                      : e.weight_diff ?? 0}
                                    )
                                  </span>
                                )}
                              </div>
                            </td>

                            {/* AI総合偏差値 */}
                            <td className="py-3 px-3">
                              <div className="flex items-baseline justify-between mb-1">
                                <span className="text-base font-black text-emerald-400 font-mono">
                                  {p.total_index.toFixed(1)}
                                </span>
                                <span className="text-[10px] text-slate-500">
                                  #{p.rank_order}位
                                </span>
                              </div>
                              <div className="w-full h-2 bg-slate-950 rounded-full overflow-hidden border border-slate-800">
                                <div
                                  className={`h-full rounded-full ${
                                    p.total_index >= 62
                                      ? "bg-gradient-to-r from-emerald-500 to-teal-400"
                                      : p.total_index >= 52
                                      ? "bg-sky-500"
                                      : "bg-slate-600"
                                  }`}
                                  style={{
                                    width: `${Math.min(
                                      100,
                                      Math.max(10, ((p.total_index - 30) / 45) * 100),
                                    )}%`,
                                  }}
                                />
                              </div>
                            </td>

                            {/* 5項目スコア */}
                            <td className="py-3 px-3">
                              <div className="grid grid-cols-5 gap-1 text-center font-mono text-[11px]">
                                <div className="bg-slate-950 border border-slate-800 rounded py-1">
                                  <div className="text-[9px] text-slate-500">能力</div>
                                  <div className="font-bold text-slate-200">
                                    {p.ability_score.toFixed(0)}
                                  </div>
                                </div>
                                <div className="bg-slate-950 border border-slate-800 rounded py-1">
                                  <div className="text-[9px] text-slate-500">時計</div>
                                  <div className="font-bold text-slate-200">
                                    {p.speed_score.toFixed(0)}
                                  </div>
                                </div>
                                <div className="bg-slate-950 border border-slate-800 rounded py-1">
                                  <div className="text-[9px] text-slate-500">適性</div>
                                  <div className="font-bold text-slate-200">
                                    {p.aptitude_score.toFixed(0)}
                                  </div>
                                </div>
                                <div className="bg-slate-950 border border-slate-800 rounded py-1">
                                  <div className="text-[9px] text-slate-500">騎手</div>
                                  <div className="font-bold text-slate-200">
                                    {p.jockey_frame_score.toFixed(0)}
                                  </div>
                                </div>
                                <div className="bg-slate-950 border border-slate-800 rounded py-1">
                                  <div className="text-[9px] text-slate-500">展開</div>
                                  <div className="font-bold text-slate-200">
                                    {p.pace_score.toFixed(0)}
                                  </div>
                                </div>
                              </div>
                            </td>

                            {/* 勝率 / 複勝率 */}
                            <td className="py-3 px-3 text-right font-mono">
                              <div className="font-bold text-slate-100">
                                {(p.win_prob * 100).toFixed(1)}%
                              </div>
                              <div className="text-[11px] text-slate-400">
                                3着内 {(p.top3_prob * 100).toFixed(1)}%
                              </div>
                            </td>

                            {/* 単勝オッズ */}
                            <td className="py-3 px-3 text-right font-mono">
                              {e.win_odds ? (
                                <>
                                  <div
                                    className={`font-bold text-sm ${
                                      e.win_odds < 10
                                        ? "text-amber-400"
                                        : "text-slate-200"
                                    }`}
                                  >
                                    {e.win_odds.toFixed(1)}倍
                                  </div>
                                  {e.popularity && (
                                    <div className="text-[11px] text-slate-400">
                                      {e.popularity}番人気
                                    </div>
                                  )}
                                </>
                              ) : (
                                <span className="text-slate-500">発売前</span>
                              )}
                            </td>

                            {/* 期待値 / 妙味 */}
                            <td className="py-3 px-3 text-center font-mono">
                              <div className="flex items-center justify-center gap-1.5">
                                <span
                                  className={`px-1.5 py-0.5 rounded text-[11px] font-black ${
                                    p.value_rank === "S"
                                      ? "bg-purple-500 text-white"
                                      : p.value_rank === "A"
                                      ? "bg-emerald-500 text-slate-950"
                                      : p.value_rank === "B"
                                      ? "bg-slate-700 text-slate-200"
                                      : "bg-slate-800 text-slate-500"
                                  }`}
                                >
                                  {p.value_rank}
                                </span>
                                <span
                                  className={`font-bold ${
                                    p.expected_value >= 1.15
                                      ? "text-emerald-400"
                                      : p.expected_value >= 0.9
                                      ? "text-slate-200"
                                      : "text-slate-500"
                                  }`}
                                >
                                  {p.expected_value.toFixed(2)}
                                </span>
                              </div>
                            </td>

                            {/* AI評価ポイント */}
                            <td className="py-3 px-3">
                              <div className="flex flex-wrap gap-1">
                                {p.reasons.length > 0 ? (
                                  p.reasons.map((r, idx) => (
                                    <span
                                      key={idx}
                                      className="px-2 py-0.5 rounded-md bg-slate-800 border border-slate-700 text-[10px] text-slate-200"
                                    >
                                      {r}
                                    </span>
                                  ))
                                ) : (
                                  <span className="text-slate-600 text-[11px]">-</span>
                                )}
                              </div>
                            </td>

                            {/* 確定着順 */}
                            {prediction.race_info.is_finished && (
                              <td className="py-3 px-3 text-center">
                                {e.actual_rank ? (
                                  <div className="flex flex-col items-center">
                                    <span
                                      className={`px-2 py-0.5 rounded-md font-black text-xs ${
                                        e.actual_rank === 1
                                          ? "bg-amber-400 text-slate-950"
                                          : e.actual_rank === 2
                                          ? "bg-slate-300 text-slate-950"
                                          : e.actual_rank === 3
                                          ? "bg-amber-700 text-white"
                                          : "bg-slate-800 text-slate-400"
                                      }`}
                                    >
                                      {e.actual_rank}着
                                    </span>
                                    {e.actual_time && (
                                      <span className="text-[10px] text-slate-400 font-mono mt-0.5">
                                        {e.actual_time}
                                      </span>
                                    )}
                                  </div>
                                ) : (
                                  <span className="text-slate-600">-</span>
                                )}
                              </td>
                            )}

                            {/* Expand button */}
                            <td className="py-3 px-2 text-center text-slate-400">
                              {isExpanded ? (
                                <ChevronUp className="w-4 h-4 mx-auto text-emerald-400" />
                              ) : (
                                <ChevronDown className="w-4 h-4 mx-auto" />
                              )}
                            </td>
                          </tr>

                          {/* Expanded Past 5 Runs Drawer */}
                          {isExpanded && (
                            <tr className="bg-slate-950/95 border-b border-slate-800">
                              <td
                                colSpan={prediction.race_info.is_finished ? 13 : 12}
                                className="p-4"
                              >
                                <div className="space-y-2">
                                  <div className="flex items-center justify-between">
                                    <span className="text-xs font-bold text-emerald-400 flex items-center gap-1.5">
                                      <History className="w-3.5 h-3.5" />
                                      {e.umaban}番 {e.horse_name} の過去走パフォーマンス詳細（直近
                                      {e.past_runs.length}走）
                                    </span>
                                    <span className="text-[11px] text-slate-400">
                                      ローテーション: {e.interval_text || "不明"} / 母:{e.dam}
                                    </span>
                                  </div>

                                  {e.past_runs.length === 0 ? (
                                    <div className="text-xs text-slate-500 py-2">
                                      過去走データがありません（新馬または初出走）
                                    </div>
                                  ) : (
                                    <div className="grid grid-cols-1 md:grid-cols-5 gap-2.5">
                                      {e.past_runs.map((run, rIdx) => (
                                        <div
                                          key={rIdx}
                                          className={`rounded-xl p-3 border text-xs space-y-1 ${
                                            run.finish_rank === 1
                                              ? "bg-amber-500/10 border-amber-500/40"
                                              : run.finish_rank && run.finish_rank <= 3
                                              ? "bg-emerald-500/10 border-emerald-500/30"
                                              : "bg-slate-900 border-slate-800"
                                          }`}
                                        >
                                          <div className="flex items-center justify-between text-[11px] text-slate-400">
                                            <span>
                                              {run.date} {run.venue}
                                            </span>
                                            <span
                                              className={`px-1.5 py-0.2 rounded font-black ${
                                                run.finish_rank === 1
                                                  ? "bg-amber-400 text-slate-950"
                                                  : run.finish_rank && run.finish_rank <= 3
                                                  ? "bg-emerald-500 text-slate-950"
                                                  : "bg-slate-800 text-slate-300"
                                              }`}
                                            >
                                              {run.finish_rank ?? "-"}着
                                            </span>
                                          </div>
                                          <div className="font-bold text-slate-100 flex items-center gap-1 truncate">
                                            <span className="truncate">{run.race_name}</span>
                                            {run.grade && (
                                              <span className="px-1 py-0.1 bg-slate-800 text-amber-300 rounded text-[10px] shrink-0">
                                                {run.grade}
                                              </span>
                                            )}
                                          </div>
                                          <div className="flex items-center justify-between text-[11px] text-slate-300 font-mono">
                                            <span>
                                              {run.surface}
                                              {run.distance}m ({run.track_cond})
                                            </span>
                                            <span>{run.time_str}</span>
                                          </div>
                                          <div className="text-[11px] text-slate-400 flex items-center justify-between">
                                            <span>
                                              通過: {run.passage || "-"} / 上り:{" "}
                                              <strong className="text-emerald-300">
                                                {run.last_3f ? `${run.last_3f}s` : "-"}
                                              </strong>
                                            </span>
                                            <span>
                                              着差:{" "}
                                              {run.margin !== null
                                                ? `${run.margin > 0 ? "+" : ""}${run.margin}s`
                                                : "-"}
                                            </span>
                                          </div>
                                          <div className="text-[10px] text-slate-500">
                                            {run.head_count ?? "-"}頭 {run.horse_num ?? "-"}番 (
                                            {run.popularity ?? "-"}人) {run.jockey}{" "}
                                            {run.carried_weight ? `${run.carried_weight}kg` : ""}
                                          </div>
                                        </div>
                                      ))}
                                    </div>
                                  )}
                                </div>
                              </td>
                            </tr>
                          )}
                        </React.Fragment>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        )}

        {/* ================= TAB 3: BETTING & EV SIMULATOR ================= */}
        {activeTab === "betting" && prediction && (
          <div className="space-y-5">
            {/* Simulator Config Header */}
            <div className="bg-slate-900 border border-slate-800 rounded-2xl p-5 flex flex-col lg:flex-row items-start lg:items-center justify-between gap-4">
              <div>
                <h2 className="text-base font-bold text-white flex items-center gap-2">
                  <Coins className="w-5 h-5 text-emerald-400" />
                  買い目推奨・期待値＆資金配分シミュレーター ({prediction.race_info.venue}
                  {prediction.race_info.race_num} {prediction.race_info.race_name})
                </h2>
                <p className="text-xs text-slate-400 mt-0.5">
                  AI推定勝率（Harville公式）とオッズから合成期待値を算出し、予算に応じた最適購入金額（100円単位）を提案します。
                </p>
              </div>

              <div className="flex flex-wrap items-center gap-3">
                {/* Budget Selector */}
                <div className="flex items-center gap-1.5 bg-slate-950 border border-slate-800 rounded-xl p-1.5">
                  <span className="text-xs text-slate-400 px-2">軍資金:</span>
                  {[1000, 3000, 5000, 10000].map((b) => (
                    <button
                      key={b}
                      onClick={() => {
                        setBudget(b);
                        handleRecalculate(weights, b, strategy);
                      }}
                      className={`px-2.5 py-1 rounded-lg text-xs font-bold transition cursor-pointer ${
                        budget === b
                          ? "bg-emerald-600 text-white"
                          : "text-slate-400 hover:text-slate-200"
                      }`}
                    >
                      {b.toLocaleString()}円
                    </button>
                  ))}
                </div>

                {/* Strategy Selector */}
                <div className="flex items-center gap-1.5 bg-slate-950 border border-slate-800 rounded-xl p-1.5">
                  <span className="text-xs text-slate-400 px-2">配分戦略:</span>
                  {[
                    { id: "balance", label: "バランス配分" },
                    { id: "hit_focus", label: "的中率重視" },
                    { id: "value_focus", label: "高期待値・妙味重視" },
                  ].map((st) => (
                    <button
                      key={st.id}
                      onClick={() => {
                        setStrategy(st.id);
                        handleRecalculate(weights, budget, st.id);
                      }}
                      className={`px-2.5 py-1 rounded-lg text-xs font-bold transition cursor-pointer ${
                        strategy === st.id
                          ? "bg-emerald-600 text-white"
                          : "text-slate-400 hover:text-slate-200"
                      }`}
                    >
                      {st.label}
                    </button>
                  ))}
                </div>

                <button
                  onClick={handleSaveToDb}
                  className="px-4 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold flex items-center gap-1.5 cursor-pointer"
                >
                  <Save className="w-4 h-4" />
                  この買い目で保存
                </button>
              </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-3 gap-5">
              {/* Recommended Tickets Table */}
              <div className="lg:col-span-2 bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden shadow-lg">
                <div className="px-5 py-3.5 border-b border-slate-800 flex items-center justify-between">
                  <h3 className="font-bold text-sm text-white flex items-center gap-2">
                    <Target className="w-4 h-4 text-emerald-400" />
                    AI推奨馬券フォーメーション（計 {prediction.recommendations.length} 点 /{" "}
                    {prediction.total_bet.toLocaleString()}円）
                  </h3>
                  {prediction.race_info.is_finished && (
                    <div className="flex items-center gap-3 text-xs">
                      <span>
                        払戻合計:{" "}
                        <strong className="text-emerald-400 text-sm">
                          {(prediction.total_return ?? 0).toLocaleString()}円
                        </strong>
                      </span>
                      <span
                        className={`px-2 py-0.5 rounded font-black ${
                          (prediction.roi_percent ?? 0) >= 100
                            ? "bg-emerald-500/20 text-emerald-300 border border-emerald-500/40"
                            : "bg-slate-800 text-slate-300"
                        }`}
                      >
                        回収率 {prediction.roi_percent?.toFixed(1)}%
                      </span>
                    </div>
                  )}
                </div>

                <table className="w-full text-left border-collapse text-xs">
                  <thead>
                    <tr className="bg-slate-950/80 border-b border-slate-800 text-[11px] text-slate-400">
                      <th className="py-2.5 px-4">券種</th>
                      <th className="py-2.5 px-4">買い目 (馬番)</th>
                      <th className="py-2.5 px-4">対象馬名 / 位置付け</th>
                      <th className="py-2.5 px-3 text-right">推定的中率</th>
                      <th className="py-2.5 px-3 text-right">想定オッズ</th>
                      <th className="py-2.5 px-3 text-center">期待値</th>
                      <th className="py-2.5 px-4 text-right">推奨購入額</th>
                      {prediction.race_info.is_finished && (
                        <th className="py-2.5 px-4 text-right">確定結果・払戻</th>
                      )}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/70">
                    {prediction.recommendations.map((rec, i) => (
                      <tr
                        key={i}
                        className={
                          rec.is_hit
                            ? "bg-emerald-950/30 hover:bg-emerald-950/40"
                            : "hover:bg-slate-800/40"
                        }
                      >
                        <td className="py-3 px-4">
                          <span className="px-2.5 py-1 rounded-md bg-slate-800 text-slate-200 font-bold">
                            {rec.bet_type}
                          </span>
                        </td>
                        <td className="py-3 px-4 font-mono font-black text-sm text-white">
                          {rec.combination_text}
                        </td>
                        <td className="py-3 px-4">
                          <div className="font-semibold text-slate-200">
                            {rec.horse_names.join(" × ")}
                          </div>
                          <span className="text-[10px] text-emerald-400">{rec.confidence}</span>
                        </td>
                        <td className="py-3 px-3 text-right font-mono text-slate-300">
                          {(rec.est_hit_prob * 100).toFixed(1)}%
                        </td>
                        <td className="py-3 px-3 text-right font-mono text-amber-300">
                          {rec.est_odds.toFixed(1)}倍
                        </td>
                        <td className="py-3 px-3 text-center font-mono">
                          <span
                            className={`px-2 py-0.5 rounded font-bold ${
                              rec.expected_value >= 1.1
                                ? "bg-emerald-500/20 text-emerald-300"
                                : "bg-slate-800 text-slate-300"
                            }`}
                          >
                            {rec.expected_value.toFixed(2)}
                          </span>
                        </td>
                        <td className="py-3 px-4 text-right font-mono font-extrabold text-sm text-white">
                          {rec.suggested_amount.toLocaleString()}円
                        </td>
                        {prediction.race_info.is_finished && (
                          <td className="py-3 px-4 text-right font-mono">
                            {rec.is_hit ? (
                              <div>
                                <span className="px-2 py-0.5 rounded bg-emerald-500 text-slate-950 font-black text-[11px] mr-1.5">
                                  的中!
                                </span>
                                <strong className="text-emerald-400 text-sm">
                                  {(rec.actual_return ?? 0).toLocaleString()}円
                                </strong>
                                <div className="text-[10px] text-slate-400">
                                  (100円あたり {rec.actual_payout_per_100?.toLocaleString()}円)
                                </div>
                              </div>
                            ) : (
                              <span className="text-slate-600">不的中 (0円)</span>
                            )}
                          </td>
                        )}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {/* Official JRA Payouts Panel */}
              <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden shadow-lg flex flex-col">
                <div className="px-5 py-3.5 border-b border-slate-800">
                  <h3 className="font-bold text-sm text-white flex items-center gap-2">
                    <Award className="w-4 h-4 text-amber-400" />
                    公式確定払戻一覧
                  </h3>
                </div>
                {prediction.official_payouts.length === 0 ? (
                  <div className="p-8 text-center text-xs text-slate-500 flex-1 flex flex-col items-center justify-center gap-2">
                    <Info className="w-6 h-6 text-slate-600" />
                    <p>
                      レース確定後に「オッズ・結果更新」ボタンを押すと、公式払戻金額と推奨馬券の自動答え合わせが表示されます。
                    </p>
                  </div>
                ) : (
                  <div className="divide-y divide-slate-800/70 text-xs">
                    {prediction.official_payouts.map((pay, idx) => (
                      <div
                        key={idx}
                        className="px-4 py-2.5 flex items-center justify-between hover:bg-slate-800/40"
                      >
                        <div className="flex items-center gap-3">
                          <span className="w-14 px-2 py-0.5 rounded bg-slate-800 text-slate-300 text-center font-bold">
                            {pay.bet_type}
                          </span>
                          <span className="font-mono font-bold text-white text-sm">
                            {pay.combination_text}
                          </span>
                        </div>
                        <div className="text-right font-mono">
                          <span className="font-extrabold text-amber-400 text-sm">
                            {pay.payout_yen.toLocaleString()}円
                          </span>
                          {pay.popularity && (
                            <span className="text-[11px] text-slate-500 ml-2">
                              ({pay.popularity}人気)
                            </span>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* ================= TAB 4: SQLITE HISTORY & ROI DASHBOARD ================= */}
        {activeTab === "history" && dashboard && (
          <div className="space-y-5">
            {/* KPI Summary Cards */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-4 space-y-1">
                <div className="text-xs text-slate-400 flex items-center justify-between">
                  <span>保存レース数 (確定済)</span>
                  <Database className="w-4 h-4 text-emerald-400" />
                </div>
                <div className="text-2xl font-black text-white font-mono">
                  {dashboard.total_saved_races}{" "}
                  <span className="text-sm font-normal text-slate-400">
                    ({dashboard.verified_races}件確定)
                  </span>
                </div>
                <div className="text-[11px] text-slate-400">
                  馬券的中レース: {dashboard.ticket_hit_races}回
                </div>
              </div>

              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-4 space-y-1">
                <div className="text-xs text-slate-400 flex items-center justify-between">
                  <span>本命◎ 勝率 / 複勝圏率</span>
                  <Trophy className="w-4 h-4 text-amber-400" />
                </div>
                <div className="text-2xl font-black text-white font-mono">
                  {dashboard.verified_races > 0
                    ? `${((dashboard.honmei_win_count / dashboard.verified_races) * 100).toFixed(1)}%`
                    : "0.0%"}
                  <span className="text-sm font-normal text-emerald-400 ml-2">
                    / 複勝{" "}
                    {dashboard.verified_races > 0
                      ? `${((dashboard.honmei_top3_count / dashboard.verified_races) * 100).toFixed(1)}%`
                      : "0.0%"}
                  </span>
                </div>
                <div className="text-[11px] text-slate-400">
                  ◎1着: {dashboard.honmei_win_count}回 / ◎3着内: {dashboard.honmei_top3_count}回
                </div>
              </div>

              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-4 space-y-1">
                <div className="text-xs text-slate-400 flex items-center justify-between">
                  <span>累計投資 / 累計払戻</span>
                  <Coins className="w-4 h-4 text-sky-400" />
                </div>
                <div className="text-xl font-black text-white font-mono">
                  {dashboard.total_return.toLocaleString()}円
                </div>
                <div className="text-[11px] text-slate-400">
                  累計投資額: {dashboard.total_investment.toLocaleString()}円 (収支:{" "}
                  <span
                    className={
                      dashboard.net_profit >= 0 ? "text-emerald-400" : "text-rose-400"
                    }
                  >
                    {dashboard.net_profit >= 0 ? "+" : ""}
                    {dashboard.net_profit.toLocaleString()}円
                  </span>
                  )
                </div>
              </div>

              <div className="bg-slate-900 border border-slate-800 rounded-2xl p-4 space-y-1">
                <div className="text-xs text-slate-400 flex items-center justify-between">
                  <span>通算回収率 (ROI)</span>
                  <TrendingUp className="w-4 h-4 text-emerald-400" />
                </div>
                <div
                  className={`text-3xl font-black font-mono ${
                    dashboard.overall_roi >= 100 ? "text-emerald-400" : "text-amber-400"
                  }`}
                >
                  {dashboard.overall_roi.toFixed(1)}%
                </div>
                <div className="text-[11px] text-slate-400">
                  SQLiteローカルデータベース集計
                </div>
              </div>
            </div>

            {/* Saved History Table */}
            <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden shadow-lg">
              <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
                <h3 className="font-bold text-sm text-white flex items-center gap-2">
                  <BookmarkCheck className="w-4 h-4 text-emerald-400" />
                  SQLite保存済み予想・回収率履歴一覧
                </h3>
                <button
                  onClick={refreshDashboard}
                  className="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 flex items-center gap-1 cursor-pointer"
                >
                  <RefreshCw className="w-3.5 h-3.5" />
                  再読込
                </button>
              </div>

              {dashboard.recent_history.length === 0 ? (
                <div className="p-12 text-center text-slate-500 text-sm">
                  まだ保存されたレース予想がありません。出馬表予想画面の「SQLiteに予想を保存」ボタンから記録できます。
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full text-left border-collapse text-xs">
                    <thead>
                      <tr className="bg-slate-950/80 border-b border-slate-800 text-[11px] text-slate-400">
                        <th className="py-3 px-4">開催日 / 競馬場</th>
                        <th className="py-3 px-4">レース名</th>
                        <th className="py-3 px-4">コース</th>
                        <th className="py-3 px-4">本命◎</th>
                        <th className="py-3 px-4 text-right">投資額</th>
                        <th className="py-3 px-4 text-right">払戻額 (回収率)</th>
                        <th className="py-3 px-4">メモ</th>
                        <th className="py-3 px-4 text-right">操作</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800/70">
                      {dashboard.recent_history.map((item) => {
                        const roi =
                          item.total_bet > 0 && item.total_return !== null
                            ? (item.total_return / item.total_bet) * 100
                            : null;
                        return (
                          <tr key={item.race_id} className="hover:bg-slate-800/40">
                            <td className="py-3 px-4">
                              <div className="font-bold text-slate-200">
                                {item.venue} {item.race_num}
                              </div>
                              <div className="text-[11px] text-slate-400">{item.race_date}</div>
                            </td>
                            <td className="py-3 px-4">
                              <div className="flex items-center gap-1.5">
                                <span className="font-bold text-white text-sm">
                                  {item.race_name}
                                </span>
                                {item.grade && (
                                  <span className="px-1.5 py-0.2 rounded bg-amber-500 text-slate-950 font-black text-[10px]">
                                    {item.grade}
                                  </span>
                                )}
                              </div>
                              <div className="text-[10px] text-slate-500 font-mono">
                                ID: {item.race_id}
                              </div>
                            </td>
                            <td className="py-3 px-4">
                              <span className="text-slate-300">
                                {item.surface}
                                {item.distance}m
                              </span>
                            </td>
                            <td className="py-3 px-4">
                              <span className="font-bold text-rose-400">
                                ◎ {item.honmei_umaban}番 {item.honmei_name}
                              </span>
                            </td>
                            <td className="py-3 px-4 text-right font-mono text-slate-200">
                              {item.total_bet.toLocaleString()}円
                            </td>
                            <td className="py-3 px-4 text-right font-mono">
                              {item.is_verified ? (
                                <div>
                                  <strong
                                    className={
                                      (item.total_return ?? 0) >= item.total_bet
                                        ? "text-emerald-400 text-sm"
                                        : "text-slate-300"
                                    }
                                  >
                                    {(item.total_return ?? 0).toLocaleString()}円
                                  </strong>
                                  {roi !== null && (
                                    <span className="text-[11px] text-slate-400 ml-1.5">
                                      ({roi.toFixed(0)}%)
                                    </span>
                                  )}
                                </div>
                              ) : (
                                <span className="px-2 py-0.5 rounded bg-amber-500/20 text-amber-300 text-[11px]">
                                  未確定
                                </span>
                              )}
                            </td>
                            <td className="py-3 px-4 text-slate-400 max-w-xs truncate">
                              {item.memo || "-"}
                            </td>
                            <td className="py-3 px-4 text-right">
                              <div className="flex items-center justify-end gap-2">
                                <button
                                  onClick={() => handleLoadSaved(item.race_id)}
                                  className="px-2.5 py-1 rounded bg-emerald-600 hover:bg-emerald-500 text-white font-bold cursor-pointer"
                                >
                                  開く
                                </button>
                                <button
                                  onClick={() => handleVerifySavedRace(item.race_id)}
                                  className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-200 cursor-pointer"
                                  title="確定結果を取得して回収率を更新"
                                >
                                  結果照合
                                </button>
                                <button
                                  onClick={() => handleDeleteSaved(item.race_id)}
                                  className="p-1.5 rounded bg-slate-800 hover:bg-rose-900/60 text-slate-400 hover:text-rose-300 cursor-pointer"
                                  title="削除"
                                >
                                  <Trash2 className="w-3.5 h-3.5" />
                                </button>
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
