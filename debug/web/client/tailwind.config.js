/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Dark theme colors matching the reference
        bg: {
          primary: "#0a0a0a",
          secondary: "#111111",
          tertiary: "#1a1a1a",
          hover: "#222222",
        },
        border: {
          DEFAULT: "#2a2a2a",
          light: "#333333",
        },
        text: {
          primary: "#ffffff",
          secondary: "#888888",
          muted: "#555555",
        },
        accent: {
          green: "#00ff88",
          blue: "#3b82f6",
          yellow: "#facc15",
          red: "#ef4444",
          purple: "#a855f7",
          cyan: "#22d3ee",
        },
      },
      fontFamily: {
        mono: [
          "JetBrains Mono",
          "Fira Code",
          "SF Mono",
          "Consolas",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
