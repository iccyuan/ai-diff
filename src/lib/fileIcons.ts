// official-style logos from material-icon-theme (MIT), bundled as svg assets
import typescript from "material-icon-theme/icons/typescript.svg";
import javascript from "material-icon-theme/icons/javascript.svg";
import vue from "material-icon-theme/icons/vue.svg";
import react from "material-icon-theme/icons/react.svg";
import reactTs from "material-icon-theme/icons/react_ts.svg";
import html from "material-icon-theme/icons/html.svg";
import css from "material-icon-theme/icons/css.svg";
import sass from "material-icon-theme/icons/sass.svg";
import less from "material-icon-theme/icons/less.svg";
import json from "material-icon-theme/icons/json.svg";
import markdown from "material-icon-theme/icons/markdown.svg";
import rust from "material-icon-theme/icons/rust.svg";
import golang from "material-icon-theme/icons/go.svg";
import python from "material-icon-theme/icons/python.svg";
import java from "material-icon-theme/icons/java.svg";
import kotlin from "material-icon-theme/icons/kotlin.svg";
import clang from "material-icon-theme/icons/c.svg";
import cpp from "material-icon-theme/icons/cpp.svg";
import csharp from "material-icon-theme/icons/csharp.svg";
import swift from "material-icon-theme/icons/swift.svg";
import ruby from "material-icon-theme/icons/ruby.svg";
import php from "material-icon-theme/icons/php.svg";
import console from "material-icon-theme/icons/console.svg";
import powershell from "material-icon-theme/icons/powershell.svg";
import yaml from "material-icon-theme/icons/yaml.svg";
import toml from "material-icon-theme/icons/toml.svg";
import xml from "material-icon-theme/icons/xml.svg";
import database from "material-icon-theme/icons/database.svg";
import svgIcon from "material-icon-theme/icons/svg.svg";
import image from "material-icon-theme/icons/image.svg";
import lock from "material-icon-theme/icons/lock.svg";
import document from "material-icon-theme/icons/document.svg";
import pdf from "material-icon-theme/icons/pdf.svg";
import zip from "material-icon-theme/icons/zip.svg";
import settings from "material-icon-theme/icons/settings.svg";
import docker from "material-icon-theme/icons/docker.svg";
import makefile from "material-icon-theme/icons/makefile.svg";
import git from "material-icon-theme/icons/git.svg";
import nodejs from "material-icon-theme/icons/nodejs.svg";
import vite from "material-icon-theme/icons/vite.svg";
import readme from "material-icon-theme/icons/readme.svg";
import file from "material-icon-theme/icons/file.svg";

const BY_EXT: Record<string, string> = {
  ts: typescript,
  mts: typescript,
  cts: typescript,
  tsx: reactTs,
  js: javascript,
  mjs: javascript,
  cjs: javascript,
  jsx: react,
  vue,
  html,
  htm: html,
  css,
  scss: sass,
  sass,
  less,
  json,
  jsonc: json,
  md: markdown,
  markdown,
  rs: rust,
  go: golang,
  py: python,
  java,
  kt: kotlin,
  kts: kotlin,
  c: clang,
  h: clang,
  cpp,
  hpp: cpp,
  cc: cpp,
  cs: csharp,
  swift,
  rb: ruby,
  php,
  sh: console,
  bash: console,
  zsh: console,
  bat: console,
  cmd: console,
  ps1: powershell,
  yml: yaml,
  yaml,
  toml,
  xml,
  sql: database,
  svg: svgIcon,
  png: image,
  jpg: image,
  jpeg: image,
  gif: image,
  webp: image,
  ico: image,
  lock,
  txt: document,
  log: document,
  pdf,
  zip,
  gz: zip,
  "7z": zip,
  rar: zip,
  env: settings,
  ini: settings,
  conf: settings,
};

const BY_NAME: Record<string, string> = {
  dockerfile: docker,
  makefile,
  ".gitignore": git,
  ".gitattributes": git,
  ".gitmodules": git,
  "package.json": nodejs,
  "package-lock.json": nodejs,
  "cargo.toml": rust,
  "cargo.lock": rust,
  "vite.config.ts": vite,
  "vite.config.js": vite,
  "readme.md": readme,
  license: document,
};

export function fileIcon(path: string): string {
  const name = path.split("/").pop()!.toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name];
  if (name.startsWith("dockerfile")) return docker;
  const dot = name.lastIndexOf(".");
  if (dot < 0) return file;
  return BY_EXT[name.slice(dot + 1)] ?? file;
}
