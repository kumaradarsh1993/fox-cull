// Rasterize assets/icon.svg -> assets/icon-1024.png, then run:
//   npm run tauri icon assets/icon-1024.png
// which regenerates the full src-tauri/icons/* set (png/ico/icns/Store logos).
import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync } from "node:fs";

const svg = readFileSync(new URL("../assets/icon.svg", import.meta.url));
const resvg = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } });
const png = resvg.render().asPng();
writeFileSync(new URL("../assets/icon-1024.png", import.meta.url), png);
console.log("wrote assets/icon-1024.png (1024x1024)");
