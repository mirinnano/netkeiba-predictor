import { useState } from "react";
import { Database, Loader2 } from "lucide-react";
import { IngestReport, JvStatus } from "../types";

const fmtDate = (d: string | null) => (d && d.length === 8 ? `${d.slice(0, 4)}/${d.slice(4, 6)}/${d.slice(6, 8)}` : "-");

/** JRA-VAN データベース（keiba.db）の状態と、JvDump の出力の取り込み */
export function JvDataPanel({
  status,
  onImport,
}: {
  status: JvStatus | null;
  onImport: (dir: string) => Promise<IngestReport[] | null>;
}) {
  const [dir, setDir] = useState("");
  const [busy, setBusy] = useState(false);
  const [last, setLast] = useState<IngestReport[] | null>(null);
  const ready = !!status?.path && status.races > 0;

  return (
    <section className="rounded-lg border border-line bg-paper px-4 py-3">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <h2 className="flex items-center gap-1.5 text-[14px] font-black">
          <Database className="h-4 w-4" />
          JRA-VAN データ
        </h2>
        {ready ? (
          <span className="text-[12px] text-ink-muted">
            レース <span className="num text-[14px] text-ink">{status!.races.toLocaleString()}</span> ・ 出走{" "}
            <span className="num text-[14px] text-ink">{status!.runners.toLocaleString()}</span> ・ 成績は{" "}
            <span className="num text-[14px] text-ink">{fmtDate(status!.last_result_date)}</span> まで ・ 出馬表は{" "}
            <span className="num text-[14px] text-ink">{fmtDate(status!.latest_card_date)}</span> まで
          </span>
        ) : (
          <span className="text-[12px] text-ink-muted">
            まだありません。取り込むと、勝率を JRA-VAN の履歴で当てはめたモデルで計算します。
          </span>
        )}
      </div>
      <form
        className="mt-2 flex items-center gap-2"
        onSubmit={async (e) => {
          e.preventDefault();
          if (!dir.trim()) return;
          setBusy(true);
          setLast(await onImport(dir.trim()));
          setBusy(false);
        }}
      >
        <input
          value={dir}
          onChange={(e) => setDir(e.target.value)}
          placeholder="JvDump の出力フォルダ（out）のパス"
          aria-label="JvDump の出力フォルダ"
          spellCheck={false}
          className="h-8 flex-1 rounded-md border border-line bg-ground px-2 text-[12px] placeholder:text-ink-faint focus:border-shell focus:outline-none"
        />
        <button
          type="submit"
          disabled={busy || !dir.trim()}
          className="flex h-8 items-center gap-1.5 rounded-md bg-shell px-3 text-[12px] font-bold text-white hover:bg-shell-deep disabled:opacity-50"
        >
          {busy && <Loader2 className="h-4 w-4 animate-spin" />}
          取り込む
        </button>
      </form>
      <p className="mt-1 text-[11px] text-ink-faint">前回の続きから、増えた分だけを読みます。何度実行しても同じ結果になります。</p>
      {last && (
        <ul className="mt-2 text-[11px] text-ink-muted">
          {last.map((r) => (
            <li key={r.file}>
              {r.file}: 反映 <span className="num text-[13px] text-ink">{r.upserted.toLocaleString()}</span> 件
              {r.deleted > 0 && ` ・ 削除 ${r.deleted} 件`}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
