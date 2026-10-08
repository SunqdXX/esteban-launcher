import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Profile = "clean" | "hacks";

export interface GameVersion {
  id: string;
  tag: string;
}

export interface Overview {
  versions: GameVersion[];
  gameVersion: string;
  profile: Profile;
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
  profile: Profile;
  gameVersion: string;
  installed: boolean;
  loaderVersion: string | null;
  mods: ModRow[];
  extras: ExtraJar[];
  unmanaged: string[];
  folder: string;
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
  profile: Profile;
}

export const api = {
  overview: () => invoke<Overview>("overview"),
  select: (s: Selection) => invoke<null>("select", { gameVersion: s.gameVersion, profile: s.profile }),
  acceptHacksWarning: () => invoke<null>("accept_hacks_warning"),
  status: (s: Selection) => invoke<InstanceStatus>("instance_status", { gameVersion: s.gameVersion, profile: s.profile }),
  setMod: (s: Selection, slug: string, enabled: boolean) =>
    invoke<InstanceStatus>("set_mod", { gameVersion: s.gameVersion, profile: s.profile, slug, enabled }),
  install: (s: Selection) => invoke<InstallSummary>("install", { gameVersion: s.gameVersion, profile: s.profile }),
  openFolder: (s: Selection) => invoke<null>("open_folder", { gameVersion: s.gameVersion, profile: s.profile }),
  settings: () => invoke<LauncherSettings>("launcher_settings"),
  setMemory: (memoryMb: number | null) => invoke<null>("set_memory", { memoryMb }),
  setJvmArgs: (text: string) => invoke<string>("set_jvm_args", { text }),
  checkJava: (path: string) => invoke<string>("check_java", { path }),
  setJava: (path: string | null) => invoke<string | null>("set_java", { path }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  pickJava: () => invoke<string | null>("pick_java"),
  setDataDir: (path: string | null) => invoke<string>("set_data_dir", { path }),
  packs: (action: "link" | "import", from: string, s: Selection) =>
    invoke<PackLine[]>("packs", { action, from, gameVersion: s.gameVersion, profile: s.profile }),
  about: () => invoke<About>("about"),
  openLink: (which: "github" | "esteban" | "discord") => invoke<null>("open_link", { which }),
};

export function gigabytes(mb: number): string {
  return `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB`;
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

export const MODE_LABEL: Record<Profile, string> = { clean: "Normal", hacks: "Hacked" };

export function listTitles(titles: string[]): string {
  if (titles.length <= 1) return titles.join("");
  return `${titles.slice(0, -1).join(", ")} and ${titles.at(-1) ?? ""}`;
}
