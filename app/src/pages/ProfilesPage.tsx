import { useEffect, useState } from "react";
import { api, message, type InstanceStatus, type Profile, type Selection } from "../api";
import PixelIcon, { FOLDER } from "../components/PixelIcon";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./ProfilesPage.module.css";

interface ProfilesPageProps {
  launcher: Launcher;
  onPlay: () => void;
}

const CARDS: { profile: Profile; name: string; text: string }[] = [
  {
    profile: "clean",
    name: "Normal",
    text: "Fabric and the performance mods. No cheat code, the hacks mod is never downloaded into it.",
  },
  {
    profile: "hacks",
    name: "Hacked",
    text: "Everything in Normal plus the Esteban hacks mod, in its own folders. Most servers ban it.",
  },
];

const key = (s: Selection) => `${s.profile}-${s.gameVersion}`;

export default function ProfilesPage({ launcher, onPlay }: ProfilesPageProps) {
  const { overview, selection, installing, revision } = launcher;
  const [statuses, setStatuses] = useState<Record<string, InstanceStatus>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!overview) return;
    let cancelled = false;
    const targets = CARDS.flatMap((c) => overview.versions.map((v) => ({ gameVersion: v.id, profile: c.profile })));
    Promise.all(targets.map((t) => api.status(t)))
      .then((list) => {
        if (cancelled) return;
        const next: Record<string, InstanceStatus> = {};
        for (const s of list) next[key(s)] = s;
        setStatuses(next);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(message(e));
      });
    return () => {
      cancelled = true;
    };
  }, [overview, revision]);

  if (!overview || !selection) return null;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Profiles</h1>
        <p className={styles.sub}>Each mode keeps its own folder for every version: worlds, settings and mods stay apart.</p>
      </header>
      {error && <p className={styles.error}>{error}</p>}
      <div className={styles.cards}>
        {CARDS.map((card) => (
          <section key={card.profile} className={cx(styles.card, card.profile === "hacks" && styles.hacked)}>
            <div className={styles.cardHead}>
              <h2 className={styles.name}>
                <span className={styles.dot} />
                {card.name}
              </h2>
              {card.profile === "clean" ? (
                <span className={styles.chip}>Default</span>
              ) : (
                <span className={cx(styles.chip, overview.hacksWarningAccepted && styles.chipOn)}>
                  {overview.hacksWarningAccepted ? "Warning accepted" : "Warning shows first"}
                </span>
              )}
            </div>
            <p className={styles.text}>{card.text}</p>
            <ul className={styles.rows}>
              {overview.versions.map((v) => {
                const target = { gameVersion: v.id, profile: card.profile };
                const s = statuses[key(target)];
                const current = selection.gameVersion === v.id && selection.profile === card.profile;
                const busy = installing?.gameVersion === v.id && installing.profile === card.profile;
                const mods = s ? s.mods.filter((m) => m.installed).length + s.extras.length : 0;
                return (
                  <li key={v.id} className={cx(styles.row, current && styles.current)}>
                    <span className={styles.v}>{v.id}</span>
                    <span className={styles.tag}>{v.tag}</span>
                    <span className={styles.state}>
                      {busy
                        ? "Installing"
                        : !s
                          ? ""
                          : s.installed
                            ? `Fabric ${s.loaderVersion ?? ""} · ${String(mods)} mods`
                            : "Not installed"}
                    </span>
                    <button
                      type="button"
                      className={styles.icon}
                      aria-label={`Open the ${card.name} ${v.id} folder`}
                      title="Open folder"
                      disabled={!s?.installed}
                      onClick={() => {
                        api.openFolder(target).catch((e: unknown) => {
                          setError(message(e));
                        });
                      }}
                    >
                      <PixelIcon cells={FOLDER} />
                    </button>
                    <button
                      type="button"
                      className={styles.use}
                      disabled={current}
                      onClick={() => {
                        launcher.select(target);
                        onPlay();
                      }}
                    >
                      {current ? "Selected" : "Use"}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
