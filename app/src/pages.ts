export const PAGES = ["play", "profiles", "mods", "servers", "settings", "about"] as const;

export type PageId = (typeof PAGES)[number];

export const PAGE_LABELS: Record<PageId, string> = {
  play: "Play",
  profiles: "Profiles",
  mods: "Mods",
  servers: "Servers",
  settings: "Settings",
  about: "About",
};

export const DISCLAIMER =
  "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.";
