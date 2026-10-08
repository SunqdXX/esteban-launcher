import { useState } from "react";
import { api, message } from "../api";
import ModeSwitch from "../components/ModeSwitch";
import PixelIcon, { FOLDER } from "../components/PixelIcon";
import Toggle from "../components/Toggle";
import VersionPicker from "../components/VersionPicker";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./ModsPage.module.css";

interface ModsPageProps {
  launcher: Launcher;
}

export default function ModsPage({ launcher }: ModsPageProps) {
  const { overview, selection, status, installing } = launcher;
  const [error, setError] = useState<string | null>(null);
  if (!overview || !selection) return null;
  const busy = installing?.gameVersion === selection.gameVersion && installing.profile === selection.profile;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div>
          <h1 className={styles.title}>Mods</h1>
          <p className={styles.sub}>Downloaded from Modrinth and checked against their hashes. Changes apply on the next Play.</p>
        </div>
        <div className={styles.pick}>
          <VersionPicker
            versions={overview.versions}
            value={selection.gameVersion}
            compact
            onChange={(gameVersion) => {
              setError(null);
              launcher.select({ ...selection, gameVersion });
            }}
          />
          <ModeSwitch
            value={selection.profile}
            compact
            onChange={(profile) => {
              setError(null);
              launcher.select({ ...selection, profile });
            }}
          />
        </div>
      </header>
      {error && <p className={styles.error}>{error}</p>}
      {status && !status.installed && (
        <p className={styles.banner}>Not installed yet. Toggles still work, the first Play picks the builds.</p>
      )}
      <ul className={styles.list}>
        {status?.mods.map((m) => {
          let state: string;
          let tone: string | undefined;
          if (m.skipped && !m.installed && m.wanted) {
            state = m.skipped;
            tone = styles.skipped;
          } else if (m.wanted && !m.installed) state = status.installed ? "Turns on at the next Play" : "Picked on the first Play";
          else if (!m.wanted && m.installed) state = "Turns off at the next Play";
          else if (!m.wanted) state = "Off";
          else state = m.version ?? "";
          return (
            <li key={m.slug} className={cx(styles.row, !m.wanted && styles.off)}>
              <div className={styles.what}>
                <span className={styles.name}>{m.title}</span>
                <span className={cx(styles.state, tone)}>{state}</span>
              </div>
              {m.required ? (
                <span className={styles.badge}>Required</span>
              ) : !m.managed ? (
                <span className={styles.badge}>Needed by another mod</span>
              ) : (
                <Toggle
                  on={m.wanted}
                  label={`${m.title} ${m.wanted ? "on" : "off"}`}
                  disabled={busy}
                  onChange={(on) => {
                    setError(null);
                    void launcher.setMod(m.slug, on).then(setError);
                  }}
                />
              )}
            </li>
          );
        })}
        {status?.extras.map((name) => (
          <li key={name} className={cx(styles.row, styles.hackedRow)}>
            <div className={styles.what}>
              <span className={styles.name}>Esteban hacks mod</span>
              <span className={styles.state}>{name}</span>
            </div>
            <span className={cx(styles.badge, styles.redBadge)}>Hacked only</span>
          </li>
        ))}
      </ul>
      {status && status.unmanaged.length > 0 && (
        <section className={styles.extra}>
          <h2 className={styles.h2}>Added by you</h2>
          <p className={styles.sub}>Jars you put in the mods folder yourself. The launcher leaves them alone and doesn't check them.</p>
          <ul className={styles.files}>
            {status.unmanaged.map((name) => (
              <li key={name}>{name}</li>
            ))}
          </ul>
        </section>
      )}
      <footer className={styles.footer}>
        <button
          type="button"
          className={styles.folder}
          disabled={!status?.installed}
          onClick={() => {
            api.openFolder(selection).catch((e: unknown) => {
              setError(message(e));
            });
          }}
        >
          <PixelIcon cells={FOLDER} />
          Open folder
        </button>
        {status && <span className={styles.path}>{status.folder}</span>}
      </footer>
    </div>
  );
}
