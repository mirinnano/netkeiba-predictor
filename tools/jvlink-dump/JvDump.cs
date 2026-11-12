// JV-Link から JV-Data のレコードを取り出し、加工せずにファイルへ保存する。
// 解釈（各項目の位置）は Mac 側で JV-Data 仕様書に従って行う。
//
// Windows 付属の C# コンパイラ（.NET Framework 4 / C# 5）でビルドできるよう、新しい文法は使っていない。
// JV-Link は 32 ビットの COM なので、必ず x86 でビルドして実行する（build.bat 参照）。
//
//   JvDump setup                                   利用キーなどの設定画面を開く
//   JvDump open <データ種別> <開始日時> <オプション> <出力先>
//       例: JvDump open RACE 20210101000000 4 out
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
        if (rc < 0)
        {
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
        Dictionary<string, StreamWriter> writers = new Dictionary<string, StreamWriter>();
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
                    StreamWriter w;
                    if (!writers.TryGetValue(spec, out w))
                    {
                        // 位置がバイト単位で決まっている形式なので、Shift_JIS のまま書く
                        w = new StreamWriter(Path.Combine(dir, spec + ".txt"), true, Sjis);
                        writers[spec] = w;
                    }
                    w.Write(buff.EndsWith("\n") ? buff : buff + "\r\n");
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
            foreach (StreamWriter w in writers.Values)
            {
                w.Dispose();
            }
            jv.JVClose();
        }
        Console.WriteLine();
        Console.WriteLine("レコード数 " + records);
        // 次回はこの時刻から差分だけ取れる
        File.WriteAllText(Path.Combine(dir, "lastfiletimestamp.txt"), lastTimestamp);
        return exitCode;
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
        using (StreamWriter w = new StreamWriter(Path.Combine(dir, key + ".txt"), false, Sjis))
        {
            while (true)
            {
                string buff = "";
                int size = 110000;
                string fileName = "";
                int r = jv.JVRead(ref buff, ref size, ref fileName);
                if (r > 0)
                {
                    w.Write(buff.EndsWith("\n") ? buff : buff + "\r\n");
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
