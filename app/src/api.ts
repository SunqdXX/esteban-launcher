import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Loader = "vanilla" | "fabric" | "forge";

export const LOADERS: Loader[] = ["vanilla", "fabric", "forge"];

export const LOADER_LABEL: Record<Loader, string> = { vanilla: "Vanilla", fabric: "Fabric", forge: "Forge" };

export interface PinnedVersion {
  id: string;
  tag: string;
}

export interface Offer {
  available: boolean;
  reason: string | null;
}

export interface Release {
  id: string;
  date: string;
  tag: string;
  pinned: boolean;
  fabric: Offer;
  forge: Offer;
}

export interface Catalog {
  latest: string;
  releases: Release[];
  offline: boolean;
}

export interface Overview {
  pinned: PinnedVersion[];
  gameVersion: string;
  loader: Loader;
  hacked: boolean;
  hacksWarningAccepted: boolean;
  hacksWarning: string;
  disclaimer: string;
}

export interface ExtraJar {
  file: string;
  title: string;
  hacks: boolean;
}

export interface LauncherSettings {
  memoryMb: number | null;
  autoMemoryMb: number;
  minMemoryMb: number;
  maxMemoryMb: number;
  totalMemoryMb: number;
  jvmArgs: string;
  javaPath: string | null;
  dataDir: string;
  defaultDataDir: string;
  packSources: string[];
}

export interface PackLine {
  folder: string;
  text: string;
}

export interface Coin {
  name: string;
  note: string;
  address: string;
  qr: string[];
}

export interface ChannelReport {
  source: "bundled" | "github" | "saved";
  sequence: number;
  issued: string;
  expires: string;
  keyId: string | null;
  keys: number;
  notice: string | null;
}

export type UpdateCheck =
  | { kind: "noKey" }
  | { kind: "nothingPublished" }
  | { kind: "offline" }
  | { kind: "upToDate"; latest: string }
  | { kind: "available"; version: string; notes: string; keyId: string }
  | { kind: "refused"; reason: string };

export interface SelfUpdate {
  package: string | null;
  reason: string | null;
}

export interface ReleaseStatus {
  launcher: string;
  channel: ChannelReport;
  update: UpdateCheck;
  selfUpdate: SelfUpdate;
  releasesPage: string;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export interface About {
  version: string;
  disclaimer: string;
  discord: boolean;
  coins: Coin[];
}

export interface ModRow {
  slug: string;
  title: string;
  version: string | null;
  wanted: boolean;
  installed: boolean;
  required: boolean;
  managed: boolean;
  skipped: string | null;
}

export interface InstanceStatus {
  loader: Loader;
  hacked: boolean;
  gameVersion: string;
  installed: boolean;
  loaderVersion: string | null;
  loaderPinned: boolean;
  skin: string | null;
  skinMod: string | null;
  skinNote: string | null;
  hackedAllowed: boolean;
  mods: ModRow[];
  extras: ExtraJar[];
  unmanaged: string[];
  folder: string;
}

export interface LoaderVersion {
  version: string;
  label: string;
  stable: boolean;
}

export interface LoaderVersions {
  loader: Loader;
  versions: LoaderVersion[];
  default: string | null;
  note: string | null;
}

export type Model = "classic" | "slim";

export interface SkinCard {
  id: string;
  name: string;
  model: Model;
  sha256: string;
  width: number;
  height: number;
  added: number;
  preview: string[];
}

export interface InstallSummary {
  loaderVersion: string;
  javaVersion: string;
  mods: number;
  fetchedFiles: number;
  fetchedBytes: number;
  skipped: string[];
}

export interface Progress {
  stage: string;
  files: number;
  filesDone: number;
  bytes: number;
  bytesDone: number;
  notice: string | null;
  done: boolean;
}

export interface Selection {
  gameVersion: string;
  loader: Loader;
  hacked: boolean;
}

const wire = (s: Selection) => ({ selection: { gameVersion: s.gameVersion, loader: s.loader, hacked: s.hacked } });

export const api = {
  overview: () => invoke<Overview>("overview"),
  catalog: (refresh: boolean) => invoke<Catalog>("catalog", { refresh }),
  select: (s: Selection) => invoke<null>("select", wire(s)),
  acceptHacksWarning: () => invoke<null>("accept_hacks_warning"),
  status: (s: Selection) => invoke<InstanceStatus>("instance_status", wire(s)),
  setMod: (s: Selection, slug: string, enabled: boolean) => invoke<InstanceStatus>("set_mod", { ...wire(s), slug, enabled }),
  install: (s: Selection) => invoke<InstallSummary>("install", wire(s)),
  openFolder: (s: Selection) => invoke<null>("open_folder", wire(s)),
  loaderVersions: (s: Selection) => invoke<LoaderVersions>("loader_versions", wire(s)),
  setLoaderVersion: (s: Selection, version: string | null) => invoke<InstanceStatus>("set_loader_version", { ...wire(s), version }),
  instances: () => invoke<InstanceStatus[]>("instances"),
  skins: () => invoke<SkinCard[]>("skins"),
  importSkin: () => invoke<SkinCard | null>("import_skin"),
  updateSkin: (id: string, change: { name?: string; model?: Model }) =>
    invoke<SkinCard>("update_skin", { id, name: change.name ?? null, model: change.model ?? null }),
  removeSkin: (id: string) => invoke<null>("remove_skin", { id }),
  setSkin: (s: Selection, skin: string | null) => invoke<InstanceStatus>("set_skin", { ...wire(s), skin }),
  settings: () => invoke<LauncherSettings>("launcher_settings"),
  setMemory: (memoryMb: number | null) => invoke<null>("set_memory", { memoryMb }),
  setJvmArgs: (text: string) => invoke<string>("set_jvm_args", { text }),
  checkJava: (path: string) => invoke<string>("check_java", { path }),
  setJava: (path: string | null) => invoke<string | null>("set_java", { path }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  pickJava: () => invoke<string | null>("pick_java"),
  setDataDir: (path: string | null) => invoke<string>("set_data_dir", { path }),
  packs: (action: "link" | "import", from: string, s: Selection) => invoke<PackLine[]>("packs", { action, from, ...wire(s) }),
  about: () => invoke<About>("about"),
  releaseStatus: () => invoke<ReleaseStatus>("release_status"),
  installUpdate: () => invoke<{ version: string; restart: boolean }>("install_update"),
  restartApp: () => invoke<null>("restart_app"),
  openLink: (which: "github" | "esteban" | "discord" | "releases") => invoke<null>("open_link", { which }),
};

export function gigabytes(mb: number): string {
  return `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB`;
}

export function onUpdateProgress(handler: (p: UpdateProgress) => void): Promise<UnlistenFn> {
  return listen<UpdateProgress>("update-progress", (event) => {
    handler(event.payload);
  });
}

export function onProgress(handler: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("install-progress", (event) => {
    handler(event.payload);
  });
}

export function message(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

export function buildLabel(gameVersion: string, build: string | null): string {
  if (!build) return "";
  const suffix = `-${gameVersion}`;
  return build.endsWith(suffix) ? build.slice(0, -suffix.length) : build;
}

export function plural(count: number, word: string): string {
  return `${String(count)} ${word}${count === 1 ? "" : "s"}`;
}

export function selectionLabel(s: Selection): string {
  return `${LOADER_LABEL[s.loader]} ${s.gameVersion}${s.hacked ? " Hacked" : ""}`;
}

export function sameSelection(a: Selection | null, b: Selection | null): boolean {
  return !!a && !!b && a.gameVersion === b.gameVersion && a.loader === b.loader && a.hacked === b.hacked;
}

export function offerFor(release: Release | undefined, loader: Loader): Offer {
  if (loader === "vanilla") return { available: true, reason: null };
  if (!release) return { available: loader === "fabric", reason: null };
  return release[loader];
}

export function hackedAllowed(s: { gameVersion: string; loader: Loader }, pinned: PinnedVersion[]): boolean {
  return s.loader === "fabric" && pinned.some((p) => p.id === s.gameVersion);
}

export function settle(next: Selection, releases: Release[], pinned: PinnedVersion[]): Selection {
  const release = releases.find((r) => r.id === next.gameVersion);
  let loader = next.loader;
  if (release && !offerFor(release, loader).available) loader = release.fabric.available ? "fabric" : "vanilla";
  const hacked = next.hacked && hackedAllowed({ gameVersion: next.gameVersion, loader }, pinned);
  return { gameVersion: next.gameVersion, loader, hacked };
}

export function listTitles(titles: string[]): string {
  if (titles.length <= 1) return titles.join("");
  return `${titles.slice(0, -1).join(", ")} and ${titles.at(-1) ?? ""}`;
}
