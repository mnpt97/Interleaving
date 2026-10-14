// Loaded explicitly by Tailwind v4 through App.css's @config directive.
const shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];
const scale = (name) => Object.fromEntries(shades.map((shade) => [shade, `var(--${name}-${shade})`]));

export default {
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        text: scale("text"),
        background: scale("background"),
        primary: scale("primary"),
        secondary: scale("secondary"),
        accent: scale("accent"),
      },
    },
  },
};
