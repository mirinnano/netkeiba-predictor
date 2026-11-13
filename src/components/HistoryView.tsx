import { useState } from "react";
import { RefreshCw, Trash2 } from "lucide-react";
import { RoiDashboardStats } from "../types";
import { yen } from "../lib/format";

export function HistoryView({
  stats,
  busy,
  onReload,
  onOpen,
  onVerify,
  onDelete,
}: {
  stats: RoiDashboardStats;
  busy: boolean;
  onReload: () => void;
  onOpen: (raceId: string) => void;
  onVerify: (raceId: string) => void;
  onDelete: (raceId: string) => void;
}) {
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const n = stats.verified_races;
  const rate = (k: number) => (n > 0 ? `${((k / n) * 100).toFixed(1)}%` : "-");

  const figures: [string, string, string?][] = [
    ["回収率", n > 0 ? `${stats.overall_roi.toFixed(1)}%` : "-", n > 0 ? (stats.overall_roi >= 100 ? "text-hit" : "") : ""],
    ["収支", `${stats.net_profit >= 0 ? "+" : ""}${yen(stats.net_profit)}`, stats.net_profit > 0 ? "text-hit" : ""],
    ["投資", yen(stats.total_investment)],
    ["払戻", yen(stats.total_return)],
    ["◎の勝率", rate(stats.honmei_win_count)],
    ["◎の3着内率", rate(stats.honmei_top3_count)],
    ["的中レース", n > 0 ? `${stats.ticket_hit_races} / ${n}` : "-"],
  ];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-baseline gap-3">
        <h1 className="text-[24px] font-black">回収率</h1>
        <span className="text-ink-muted">
          発走前に保存し、結果照合まで済んだ {n} レースで計算
          {stats.backtest_races > 0 && `（確定後に保存した ${stats.backtest_races} 件は集計外）`}
        </span>
        <button
          type="button"
          onClick={onReload}
          className="ml-auto flex items-center gap-1.5 rounded-md border border-line bg-paper px-3 py-1.5 text-[12px] font-bold hover:border-ink"
        >
          <RefreshCw className="h-4 w-4" />
          再読込
        </button>
      </div>

      <dl className="flex flex-wrap divide-x divide-line border-y border-ink py-3">
        {figures.map(([k, v, cls]) => (
          <div key={k} className="px-5 first:pl-0">
            <dt className="text-[11px] text-ink-muted">{k}</dt>
            <dd className={`num text-[26px] font-bold leading-tight ${cls ?? ""}`}>{v}</dd>
          </div>
        ))}
      </dl>

      {stats.recent_history.length === 0 ? (
        <p className="py-10 text-center text-ink-muted">
          保存した予想はまだありません。出馬表の画面で「この予想を保存」を押すと、ここに並びます。
        </p>
      ) : (
        <div className="max-md:relative max-md:-mx-4 max-md:overflow-x-auto max-md:px-4">
        <table className="w-full border-collapse whitespace-nowrap max-md:text-[13px]">
          <thead>
            <tr className="border-b border-ink text-[11px] text-ink-muted">
              <th className="py-2 pr-3 text-left font-medium">日付・場</th>
              <th className="py-2 pr-3 text-left font-medium">レース</th>
              <th className="py-2 pr-3 text-left font-medium">◎</th>
              <th className="py-2 pr-3 text-right font-medium">投資</th>
              <th className="py-2 pr-3 text-right font-medium">払戻</th>
              <th className="py-2 pr-3 text-left font-medium">メモ</th>
              <th className="py-2 text-right font-medium">
                <span className="sr-only">操作</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {stats.recent_history.map((item) => {
              const roi =
                item.total_bet > 0 && item.total_return !== null ? (item.total_return / item.total_bet) * 100 : null;
              const excluded = item.is_verified && !item.is_pre_race;
              return (
                <tr key={item.race_id} className="border-b border-line-soft">
                  <td className="py-2 pr-3 whitespace-nowrap">
                    <div className="font-bold">
                      {item.venue} {item.race_num}
                    </div>
                    <div className="text-[11px] text-ink-muted">{item.race_date}</div>
                  </td>
                  <td className="py-2 pr-3">
                    <span className="font-bold">{item.race_name}</span>
                    {item.grade && <span className="ml-1.5 text-[11px] font-black">{item.grade}</span>}
                    <div className="text-[11px] text-ink-muted">
                      {item.surface}
                      {item.distance}m
                    </div>
                  </td>
                  <td className="py-2 pr-3 whitespace-nowrap">
                    <span className="num text-[15px] font-bold">{item.honmei_umaban}</span> {item.honmei_name}
                  </td>
                  <td className="py-2 pr-3 text-right num text-[15px]">{item.total_bet.toLocaleString()}</td>
                  <td className="py-2 pr-3 text-right whitespace-nowrap">
                    {!item.is_verified ? (
                      <span className="text-[12px] text-warn">未確定</span>
                    ) : excluded ? (
                      <span className="text-ink-faint" title="確定後に保存した予想なので回収率に含めていません">
                        <span className="num text-[15px]">{(item.total_return ?? 0).toLocaleString()}</span>
                        <span className="ml-1 text-[11px]">集計外</span>
                      </span>
                    ) : (
                      <span>
                        <span className={`num text-[15px] font-bold ${(item.total_return ?? 0) >= item.total_bet ? "text-hit" : ""}`}>
                          {(item.total_return ?? 0).toLocaleString()}
                        </span>
                        {roi !== null && <span className="ml-1 text-[11px] text-ink-muted">{roi.toFixed(0)}%</span>}
                      </span>
                    )}
                  </td>
                  <td className="py-2 pr-3 max-w-[220px] truncate text-ink-muted">{item.memo || ""}</td>
                  <td className="py-2 text-right whitespace-nowrap">
                    {pendingDelete === item.race_id ? (
                      <span className="inline-flex gap-1.5">
                        <button
                          type="button"
                          autoFocus
                          onClick={() => {
                            setPendingDelete(null);
                            onDelete(item.race_id);
                          }}
                          className="rounded-md bg-mark px-2.5 py-1 text-[12px] font-bold text-white"
                        >
                          削除する
                        </button>
                        <button
                          type="button"
                          onClick={() => setPendingDelete(null)}
                          className="rounded-md border border-line px-2.5 py-1 text-[12px] font-bold"
                        >
                          やめる
                        </button>
                      </span>
                    ) : (
                      <span className="inline-flex gap-1.5">
                        <button
                          type="button"
                          onClick={() => onOpen(item.race_id)}
                          className="rounded-md border border-line bg-paper px-2.5 py-1 text-[12px] font-bold hover:border-ink"
                        >
                          開く
                        </button>
                        {!item.is_verified && (
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => onVerify(item.race_id)}
                            className="rounded-md bg-shell px-2.5 py-1 text-[12px] font-bold text-white hover:bg-shell-deep disabled:opacity-50"
                          >
                            結果照合
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => setPendingDelete(item.race_id)}
                          aria-label={`${item.race_name} の保存データを削除`}
                          className="rounded-md p-1.5 text-ink-faint hover:bg-mark/10 hover:text-mark"
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        </div>
      )}
    </div>
  );
}
