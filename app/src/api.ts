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
  extras: string[];
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
};

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
