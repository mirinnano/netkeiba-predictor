/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        // 昼の地と墨
        ground: "#F7F6F2",
        paper: "#FFFFFF",
        ink: { DEFAULT: "#16181D", soft: "#3A3E47", muted: "#5A5F6B", faint: "#8A8F99" },
        line: { DEFAULT: "#D9D6CC", soft: "#E8E5DC" },
        // 勝負服のシェル
        shell: { DEFAULT: "#1D3FA8", deep: "#15308A", edge: "#4C67C4", text: "#C9D4F5" },
        // 印の赤インク
        mark: "#C0392B",
        // 「自分の買い目」専用の予約色。ほかの意味には使わない
        mine: { DEFAULT: "#7A2BD9", tint: "#F1E8FC" },
        // 状態
        hit: "#1F7A45",
        warn: "#9A6B00",
        // 枠帽の8色
        waku: {
          1: "#FFFFFF",
          2: "#1A1A1A",
          3: "#E0262B",
          4: "#1F5FBF",
          5: "#F2C200",
          6: "#1F8A3B",
          7: "#F07C1A",
          8: "#F06FA4",
        },
      },
      fontFamily: {
        sans: ['"Yu Gothic UI"', '"Hiragino Sans"', '"Hiragino Kaku Gothic ProN"', "Meiryo", "sans-serif"],
        num: ['"Barlow Condensed"', '"Arial Narrow"', '"Yu Gothic UI"', "sans-serif"],
      },
    },
  },
  plugins: [],
};
