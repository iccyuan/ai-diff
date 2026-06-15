// Renders text-based vector assets (cross-platform SVG and Android Vector
// Drawable) to an <img>-loadable data URL. Loading via <img> means embedded
// scripts never execute, so no sanitization is required for the preview.
//
// Android Vector Drawable (AVD) is XML with a <vector> root; it maps closely to
// SVG (paths, groups, clip-paths, gradients). iOS ships vectors as PDF in asset
// catalogs (binary, not handled here) but increasingly as plain SVG too.

const SVG_NS = "http://www.w3.org/2000/svg";
const ANDROID_NS = "http://schemas.android.com/apk/res/android";

export type VectorKind = "svg" | "avd";

/** Identify a previewable vector from its path + raw text (null = not one). */
export function vectorKind(path: string, text: string | null | undefined): VectorKind | null {
  if (text == null) return null;
  const p = path.toLowerCase();
  if (p.endsWith(".svg")) return "svg";
  // an .xml is a vector drawable only if its root element is <vector>
  if (p.endsWith(".xml") && /<\s*vector[\s>]/.test(text)) return "avd";
  return null;
}

export function isVectorSource(path: string, text: string | null | undefined): boolean {
  return vectorKind(path, text) !== null;
}

/** Data URL for an <img>, or null if it isn't a vector / can't be converted. */
export function vectorDataUrl(path: string, text: string | null | undefined): string | null {
  const kind = vectorKind(path, text);
  if (!kind || text == null) return null;
  const svg = kind === "svg" ? text : avdToSvg(text);
  if (!svg) return null;
  // base64 (UTF-8 safe) — the most webview-compatible data-URL form for <img>
  return "data:image/svg+xml;base64," + btoa(unescape(encodeURIComponent(svg)));
}

// ----- Android Vector Drawable → SVG -----

/** Read an `android:`-namespaced attribute, falling back to the raw prefix. */
function aattr(el: Element, name: string): string | null {
  return el.getAttributeNS(ANDROID_NS, name) ?? el.getAttribute("android:" + name);
}

/** First number in a string, dropping units like dp/px (null if none). */
function fnum(raw: string | null): number | null {
  if (raw == null) return null;
  const m = raw.match(/-?\d*\.?\d+(?:e-?\d+)?/i);
  return m ? parseFloat(m[0]) : null;
}

type ColorOp = { color: string; opacity: number | null };

/** AVD color (#RGB/#ARGB/#RRGGBB/#AARRGGBB/name) → CSS color + opacity. */
function parseColor(raw: string | null): ColorOp | null {
  if (raw == null) return null;
  const s = raw.trim();
  // @color/… and ?attr/… resource refs can't be resolved without the project
  if (!s || s.startsWith("@") || s.startsWith("?")) return null;
  if (s[0] === "#") {
    const h = s.slice(1);
    const a = (x: string) => parseInt(x, 16) / 255;
    if (h.length === 3 || h.length === 6) return { color: s, opacity: null };
    if (h.length === 4) return { color: `#${h[1]}${h[1]}${h[2]}${h[2]}${h[3]}${h[3]}`, opacity: a(h[0] + h[0]) };
    if (h.length === 8) return { color: `#${h.slice(2)}`, opacity: a(h.slice(0, 2)) };
  }
  return { color: s, opacity: null };
}

export function avdToSvg(xml: string): string | null {
  let doc: Document;
  try {
    doc = new DOMParser().parseFromString(xml, "application/xml");
  } catch {
    return null;
  }
  if (doc.getElementsByTagName("parsererror").length) return null;
  const root = doc.documentElement;
  if (!root || root.localName !== "vector") return null;

  const vw = fnum(aattr(root, "viewportWidth"));
  const vh = fnum(aattr(root, "viewportHeight"));
  const w = fnum(aattr(root, "width"));
  const h = fnum(aattr(root, "height"));

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("xmlns", SVG_NS);
  svg.setAttribute("viewBox", `0 0 ${vw ?? w ?? 24} ${vh ?? h ?? 24}`);
  if (w) svg.setAttribute("width", String(w));
  if (h) svg.setAttribute("height", String(h));

  const defs = document.createElementNS(SVG_NS, "defs");
  svg.appendChild(defs);
  let seq = 0;
  const uid = () => `avd${seq++}`;

  // root alpha applies to the whole drawable
  let mount: Element = svg;
  const alpha = fnum(aattr(root, "alpha"));
  if (alpha != null && alpha < 1) {
    const g = document.createElementNS(SVG_NS, "g");
    g.setAttribute("opacity", String(alpha));
    svg.appendChild(g);
    mount = g;
  }

  convertChildren(root, mount, defs, uid);

  if (!defs.childNodes.length) defs.remove();
  return new XMLSerializer().serializeToString(svg);
}

function convertChildren(parent: Element, out: Element, defs: Element, uid: () => string) {
  // clip-paths declared in a container clip everything that follows in it
  let target = out;
  for (const el of Array.from(parent.children)) {
    if (el.localName !== "clip-path") continue;
    const d = aattr(el, "pathData");
    if (!d) continue;
    const id = uid();
    const cp = document.createElementNS(SVG_NS, "clipPath");
    cp.setAttribute("id", id);
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    cp.appendChild(path);
    defs.appendChild(cp);
    const wrap = document.createElementNS(SVG_NS, "g"); // nest to intersect multiple clips
    wrap.setAttribute("clip-path", `url(#${id})`);
    target.appendChild(wrap);
    target = wrap;
  }
  for (const el of Array.from(parent.children)) {
    if (el.localName === "path") convertPath(el, target, defs, uid);
    else if (el.localName === "group") convertGroup(el, target, defs, uid);
  }
}

function convertGroup(el: Element, out: Element, defs: Element, uid: () => string) {
  const g = document.createElementNS(SVG_NS, "g");
  const tx = fnum(aattr(el, "translateX")) ?? 0;
  const ty = fnum(aattr(el, "translateY")) ?? 0;
  const sx = fnum(aattr(el, "scaleX")) ?? 1;
  const sy = fnum(aattr(el, "scaleY")) ?? 1;
  const rot = fnum(aattr(el, "rotation")) ?? 0;
  const px = fnum(aattr(el, "pivotX")) ?? 0;
  const py = fnum(aattr(el, "pivotY")) ?? 0;
  // AVD composes as translate(t+pivot) · rotate · scale · translate(-pivot)
  const t: string[] = [];
  if (tx + px !== 0 || ty + py !== 0) t.push(`translate(${tx + px} ${ty + py})`);
  if (rot !== 0) t.push(`rotate(${rot})`);
  if (sx !== 1 || sy !== 1) t.push(`scale(${sx} ${sy})`);
  if (px !== 0 || py !== 0) t.push(`translate(${-px} ${-py})`);
  if (t.length) g.setAttribute("transform", t.join(" "));
  out.appendChild(g);
  convertChildren(el, g, defs, uid);
}

function convertPath(el: Element, out: Element, defs: Element, uid: () => string) {
  const d = aattr(el, "pathData");
  if (!d) return;
  const p = document.createElementNS(SVG_NS, "path");
  p.setAttribute("d", d);

  // fill: gradient wins over a flat color; AVD's default fill is none (SVG's is black)
  const fillGrad = gradientRef(el, "fillColor", defs, uid);
  if (fillGrad) {
    p.setAttribute("fill", fillGrad);
  } else {
    const c = parseColor(aattr(el, "fillColor"));
    p.setAttribute("fill", c ? c.color : "none");
    if (c?.opacity != null) p.setAttribute("fill-opacity", String(c.opacity));
  }
  applyAlpha(p, "fill-opacity", fnum(aattr(el, "fillAlpha")));
  const fillType = aattr(el, "fillType");
  if (fillType) p.setAttribute("fill-rule", fillType.toLowerCase() === "evenodd" ? "evenodd" : "nonzero");

  // stroke
  const strokeGrad = gradientRef(el, "strokeColor", defs, uid);
  if (strokeGrad) {
    p.setAttribute("stroke", strokeGrad);
  } else {
    const c = parseColor(aattr(el, "strokeColor"));
    if (c) {
      p.setAttribute("stroke", c.color);
      if (c.opacity != null) p.setAttribute("stroke-opacity", String(c.opacity));
    }
  }
  const sw = fnum(aattr(el, "strokeWidth"));
  if (sw != null && sw > 0) p.setAttribute("stroke-width", String(sw));
  applyAlpha(p, "stroke-opacity", fnum(aattr(el, "strokeAlpha")));
  const cap = aattr(el, "strokeLineCap");
  if (cap) p.setAttribute("stroke-linecap", cap.toLowerCase());
  const join = aattr(el, "strokeLineJoin");
  if (join) p.setAttribute("stroke-linejoin", join.toLowerCase());
  const miter = fnum(aattr(el, "strokeMiterLimit"));
  if (miter != null) p.setAttribute("stroke-miterlimit", String(miter));

  out.appendChild(p);
}

/** Multiply an existing opacity attr by an AVD alpha multiplier. */
function applyAlpha(p: Element, attr: string, alpha: number | null) {
  if (alpha == null) return;
  const prev = p.getAttribute(attr);
  p.setAttribute(attr, String(prev != null ? Number(prev) * alpha : alpha));
}

/** Resolve an <aapt:attr name="android:fillColor"><gradient/> → url(#id). */
function gradientRef(el: Element, attrName: string, defs: Element, uid: () => string): string | null {
  const wrapper = Array.from(el.children).find(
    (c) => c.localName === "attr" && c.getAttribute("name") === "android:" + attrName,
  );
  const grad = wrapper && Array.from(wrapper.children).find((c) => c.localName === "gradient");
  if (!grad) return null;

  const type = (aattr(grad, "type") || "linear").toLowerCase();
  let g: Element;
  if (type === "linear") {
    g = document.createElementNS(SVG_NS, "linearGradient");
    setNum(g, "x1", aattr(grad, "startX"));
    setNum(g, "y1", aattr(grad, "startY"));
    setNum(g, "x2", aattr(grad, "endX"));
    setNum(g, "y2", aattr(grad, "endY"));
  } else if (type === "radial") {
    g = document.createElementNS(SVG_NS, "radialGradient");
    setNum(g, "cx", aattr(grad, "centerX"));
    setNum(g, "cy", aattr(grad, "centerY"));
    setNum(g, "r", aattr(grad, "gradientRadius"));
  } else {
    return null; // sweep gradients have no SVG equivalent
  }
  g.setAttribute("gradientUnits", "userSpaceOnUse");
  const id = uid();
  g.setAttribute("id", id);

  const addStop = (offset: number, raw: string | null) => {
    const c = parseColor(raw);
    if (!c) return;
    const s = document.createElementNS(SVG_NS, "stop");
    s.setAttribute("offset", String(offset));
    s.setAttribute("stop-color", c.color);
    if (c.opacity != null) s.setAttribute("stop-opacity", String(c.opacity));
    g.appendChild(s);
  };
  const items = Array.from(grad.children).filter((c) => c.localName === "item");
  if (items.length) {
    for (const it of items) addStop(fnum(aattr(it, "offset")) ?? 0, aattr(it, "color"));
  } else {
    addStop(0, aattr(grad, "startColor"));
    const center = aattr(grad, "centerColor");
    if (center != null) addStop(0.5, center);
    addStop(1, aattr(grad, "endColor"));
  }
  if (!g.childNodes.length) return null;
  defs.appendChild(g);
  return `url(#${id})`;
}

function setNum(el: Element, attr: string, raw: string | null) {
  const n = fnum(raw);
  if (n != null) el.setAttribute(attr, String(n));
}
