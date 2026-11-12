// JV-Link から JV-Data のレコードを取り出し、加工せずにファイルへ保存する。
// 解釈（各項目の位置）は Mac 側で JV-Data 仕様書に従って行う。
//
// Windows 付属の C# コンパイラ（.NET Framework 4 / C# 5）でビルドできるよう、新しい文法は使っていない。
// JV-Link は 32 ビットの COM なので、必ず x86 でビルドして実行する（build.bat 参照）。
//
//   JvDump setup                                   利用キーなどの設定画面を開く
//   JvDump open <データ種別> <開始日時> <オプション> <出力先>
//       例: JvDump open RACE 20210101000000 4 out
//   JvDump update <データ種別> <出力先>             前回の続きから差分だけ取る（通常データ）
//       例: JvDump update RACE out
//   JvDump rt <データ種別> <キー> <出力先>          速報系（時系列オッズなど）を1件取る
//       例: JvDump rt 0B41 2024111505050811 out
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;

internal static class JvDump
{
    private const string Sid = "UNKNOWN";
    private static readonly Encoding Sjis = Encoding.GetEncoding(932);

    [STAThread]
    private static int Main(string[] args)
    {
        if (args.Length == 0)
        {
            Usage();
            return 2;
        }
        Type t = Type.GetTypeFromProgID("JVDTLab.JVLink");
        if (t == null)
        {
            Console.Error.WriteLine("JV-Link が見つかりません。JV-Link をインストールしてください。");
            return 1;
        }
        if (IntPtr.Size != 4)
        {
            Console.Error.WriteLine("64 ビットで動いています。build.bat で x86 としてビルドしたものを使ってください。");
            return 1;
        }
        dynamic jv = Activator.CreateInstance(t);
        int rc = jv.JVInit(Sid);
        Console.WriteLine("JVInit=" + rc);
        if (rc != 0)
        {
            return 1;
        }

        string mode = args[0].ToLowerInvariant();
        if (mode == "setup")
        {
            rc = jv.JVSetUIProperties();
            Console.WriteLine("JVSetUIProperties=" + rc);
            return rc == 0 ? 0 : 1;
        }
        if (mode == "open" && args.Length == 5)
        {
            return OpenAndDump(jv, args[1], args[2], int.Parse(args[3]), args[4]);
        }
        if (mode == "update" && args.Length == 3)
        {
            string stampFile = Path.Combine(Path.Combine(args[2], args[1]), "lastfiletimestamp.txt");
            if (!File.Exists(stampFile))
            {
                Console.Error.WriteLine("前回の取得記録がありません: " + stampFile);
                Console.Error.WriteLine("先に open（セットアップ）で最初の取得をしてください。");
                return 1;
            }
            string fromStamp = File.ReadAllText(stampFile).Trim();
            Console.WriteLine(args[1] + ": " + fromStamp + " 以降の差分を取ります");
            // オプション 1 = 通常データ（差分）
            return OpenAndDump(jv, args[1], fromStamp, 1, args[2]);
        }
        if (mode == "rt" && args.Length == 4)
        {
            return RtDump(jv, args[1], args[2], args[3]);
        }
        Usage();
        return 2;
    }

    private static void Usage()
    {
        Console.Error.WriteLine("使い方:");
        Console.Error.WriteLine("  JvDump setup");
        Console.Error.WriteLine("  JvDump open <データ種別> <開始日時 yyyyMMddHHmmss> <オプション> <出力先>");
        Console.Error.WriteLine("  JvDump update <データ種別> <出力先>");
        Console.Error.WriteLine("  JvDump rt <データ種別> <キー> <出力先>");
    }

    /// 蓄積系データ（RACE, SLOP, WOOD, DIFN など）を取り出す
    private static int OpenAndDump(dynamic jv, string dataspec, string fromtime, int option, string outRoot)
    {
        int readCount = 0;
        int downloadCount = 0;
        string lastTimestamp = "";
        int rc = jv.JVOpen(dataspec, fromtime, option, ref readCount, ref downloadCount, ref lastTimestamp);
        Console.WriteLine("JVOpen=" + rc + " 読み込みファイル数=" + readCount + " ダウンロード数=" + downloadCount
            + " 最終ファイル時刻=" + lastTimestamp);
        if (rc == -1)
        {
            // 該当データなし（新しいデータがまだない）。時刻は進めない
            Console.WriteLine("新しいデータはありません");
            AppendLog(outRoot, dataspec, fromtime, option, 0, "no data");
            return 0;
        }
        if (rc < 0)
        {
            AppendLog(outRoot, dataspec, fromtime, option, 0, "JVOpen=" + rc);
            return 1;
        }

        // ダウンロードが終わるのを待つ
        while (downloadCount > 0)
        {
            int done = jv.JVStatus();
            if (done < 0)
            {
                Console.Error.WriteLine("JVStatus=" + done);
                jv.JVClose();
                return 1;
            }
            Console.Write("\rダウンロード " + done + " / " + downloadCount + "   ");
            if (done >= downloadCount)
            {
                Console.WriteLine();
                break;
            }
            Thread.Sleep(1000);
        }

        string dir = Path.Combine(outRoot, dataspec);
        Directory.CreateDirectory(dir);
        Dictionary<string, FileStream> writers = new Dictionary<string, FileStream>();
        long records = 0;
        int exitCode = 0;
        try
        {
            while (true)
            {
                string buff = "";
                int size = 110000;
                string fileName = "";
                int r = jv.JVRead(ref buff, ref size, ref fileName);
                if (r > 0)
                {
                    // 先頭2文字がレコード種別（RA, SE, HR, O1 など）
                    string spec = buff.Length >= 2 ? buff.Substring(0, 2) : "__";
                    FileStream w;
                    if (!writers.TryGetValue(spec, out w))
                    {
                        w = new FileStream(Path.Combine(dir, spec + ".txt"), FileMode.Append, FileAccess.Write);
                        writers[spec] = w;
                    }
                    WriteRecord(w, buff, r);
                    records++;
                    if (records % 10000 == 0)
                    {
                        Console.Write("\rレコード " + records + "   ");
                    }
                }
                else if (r == 0)
                {
                    break; // 全部読んだ
                }
                else if (r == -1)
                {
                    continue; // ファイルの切り替わり
                }
                else if (r == -3)
                {
                    Thread.Sleep(200); // まだダウンロード中
                }
                else
                {
                    Console.Error.WriteLine();
                    Console.Error.WriteLine("JVRead=" + r + "（エラー）");
                    exitCode = 1;
                    break;
                }
            }
        }
        finally
        {
            foreach (FileStream w in writers.Values)
            {
                w.Dispose();
            }
            jv.JVClose();
        }
        Console.WriteLine();
        Console.WriteLine("レコード数 " + records);
        // 最後まで読めたときだけ時刻を進める。途中で止まったのに進めると、その分を二度と取れなくなる
        if (exitCode == 0 && lastTimestamp.Length > 0)
        {
            File.WriteAllText(Path.Combine(dir, "lastfiletimestamp.txt"), lastTimestamp);
        }
        AppendLog(outRoot, dataspec, fromtime, option, records, exitCode == 0 ? "ok -> " + lastTimestamp : "read error");
        return exitCode;
    }

    /// いつ・何を・何件取ったかを out\update_log.txt に残す
    private static void AppendLog(string outRoot, string dataspec, string fromtime, int option, long records, string result)
    {
        Directory.CreateDirectory(outRoot);
        string line = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + "\t" + dataspec + "\tfrom=" + fromtime
            + "\toption=" + option + "\trecords=" + records + "\t" + result + "\r\n";
        File.AppendAllText(Path.Combine(outRoot, "update_log.txt"), line);
    }

    private static bool reportedPadding = false;

    /// JVRead の戻り値（実際に読んだバイト数）で切り詰めて、Shift_JIS のバイト列のまま書く。
    /// 受け取る文字列はバッファの大きさまで埋められて返ることがあるので、全部書くと巨大になる。
    private static void WriteRecord(FileStream w, string buff, int readBytes)
    {
        byte[] all = Sjis.GetBytes(buff);
        int n = Math.Min(readBytes, all.Length);
        if (!reportedPadding && all.Length > readBytes + 2)
        {
            Console.WriteLine();
            Console.WriteLine("（確認）受け取った長さ " + all.Length + " バイトのうち、レコードは " + readBytes + " バイト");
            reportedPadding = true;
        }
        w.Write(all, 0, n);
        if (n == 0 || all[n - 1] != (byte)'\n')
        {
            w.WriteByte((byte)'\r');
            w.WriteByte((byte)'\n');
        }
    }

    /// 速報系データ（時系列オッズなど）をキー1件ぶん取り出す
    private static int RtDump(dynamic jv, string dataspec, string key, string outRoot)
    {
        int rc = jv.JVRTOpen(dataspec, key);
        Console.WriteLine("JVRTOpen=" + rc);
        if (rc < 0)
        {
            return 1;
        }
        string dir = Path.Combine(outRoot, "rt_" + dataspec);
        Directory.CreateDirectory(dir);
        long records = 0;
        using (FileStream w = new FileStream(Path.Combine(dir, key + ".txt"), FileMode.Create, FileAccess.Write))
        {
            while (true)
            {
                string buff = "";
                int size = 110000;
                string fileName = "";
                int r = jv.JVRead(ref buff, ref size, ref fileName);
                if (r > 0)
                {
                    WriteRecord(w, buff, r);
                    records++;
                }
                else if (r == 0)
                {
                    break;
                }
                else if (r == -1)
                {
                    continue;
                }
                else
                {
                    Console.Error.WriteLine("JVRead=" + r);
                    break;
                }
            }
        }
        jv.JVClose();
        Console.WriteLine("レコード数 " + records);
        return 0;
    }
}
