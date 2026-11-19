// Read the same tokens as the workspace once, before constructing any terminals.
const styles = getComputedStyle(document.documentElement);
const token = (name) => styles.getPropertyValue("--" + name).trim();

export const terminalFont = token("terminal-font");
export const defaultFontSize = Number.parseFloat(token("type-terminal"));

export function loadTerminalFonts() {
  const family = terminalFont.split(",")[0].trim().replace(/^"|"$/g, "");
  return Promise.allSettled(
    [["Regular", "400"], ["Bold", "700"]].map(([style, weight]) => {
      const face = new FontFace(family, `url(/fonts/Minterm-Mono-${style}.woff2)`, { weight });
      document.fonts.add(face);
      return face.load();
    }),
  );
}

export const theme = {
  background: token("canvas"),
  foreground: token("term-fg"),
  cursor: token("mint"),
  cursorAccent: token("canvas"),
  selectionBackground: token("term-selection"),
};
for (const color of ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]) {
  theme[color] = token("ansi-" + color);
  theme["bright" + color[0].toUpperCase() + color.slice(1)] = token("ansi-bright-" + color);
}
