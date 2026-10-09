import { useEffect, useState } from "react";
import { LOADER_LABEL, api, hackedAllowed, message, sameSelection, type LoaderVersions } from "../api";
import Dropdown from "../components/Dropdown";
import LoaderSwitch from "../components/LoaderSwitch";
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

function LoaderBuild({ launcher }: ModsPageProps) {
  const { selection, status, installing } = launcher;
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<{ key: string; choices: LoaderVersions | null; error: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const busy = sameSelection(installing, selection);
  const key = selection ? `${selection.loader}-${selection.gameVersion}` : "";

  useEffect(() => {
    if (!open || !selection || selection.loader === "vanilla") return;
    let cancelled = false;
    api
      .loaderVersions(selection)
      .then((next) => {
        if (!cancelled) setLoaded({ key, choices: next, error: null });
      })
      .catch((e: unknown) => {
        if (!cancelled) setLoaded({ key, choices: null, error: message(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [open, selection, key]);

  const choices = loaded?.key === key ? loaded.choices : null;
  const loadError = loaded?.key === key ? loaded.error : null;
  if (!selection || !status || selection.loader === "vanilla") return null;
  const name = LOADER_LABEL[selection.loader];
  const value = status.loaderPinned ? (status.loaderVersion ?? "") : "";

  return (
    <details
      className={styles.advanced}
      onToggle={(e) => {
        setOpen(e.currentTarget.open);
      }}
    >
      <summary className={styles.summary}>Advanced</summary>
      <div className={styles.advancedBody}>
        <p className={styles.buildLabel}>{name} build</p>
        <div className={styles.build}>
          <Dropdown
            label={`${name} build`}
            value={value}
            disabled={!choices || busy}
            options={[
              { value: "", label: "Latest stable", note: choices?.default ?? null },
              ...(choices?.versions ?? []).map((v) => ({ value: v.version, label: v.label, note: v.stable ? "stable" : null })),
            ]}
            onChange={(picked) => {
              setError(null);
              api
                .setLoaderVersion(selection, picked === "" ? null : picked)
                .then(launcher.setStatus)
                .catch((err: unknown) => {
                  setError(message(err));
                });
            }}
          />
        </div>
        <p className={styles.hint}>
          {choices?.note ?? `The newest stable ${name} build is picked when nothing is set.`} A change applies on the next Play.
        </p>
        {(error ?? loadError) && <p className={styles.error}>{error ?? loadError}</p>}
      </div>
    </details>
  );
}

export default function ModsPage({ launcher }: ModsPageProps) {
  const { overview, selection, status, installing } = launcher;
  const [error, setError] = useState<string | null>(null);
  if (!overview || !selection) return null;
  const busy = sameSelection(installing, selection);
  const release = launcher.releases.find((r) => r.id === selection.gameVersion);
  const vanilla = selection.loader === "vanilla";

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div>
          <h1 className={styles.title}>Mods</h1>
          <p className={styles.sub}>Downloaded from Modrinth and checked against their hashes. Changes apply on the next Play.</p>
        </div>
        <div className={styles.pick}>
          <VersionPicker
            releases={launcher.releases}
            pinned={overview.pinned}
            value={selection.gameVersion}
            compact
            onChange={(gameVersion) => {
              setError(null);
              launcher.select({ ...selection, gameVersion });
            }}
          />
          <LoaderSwitch
            release={release}
            value={selection.loader}
            compact
            onChange={(loader) => {
              setError(null);
              launcher.select({ ...selection, loader });
            }}
          />
          {hackedAllowed(selection, overview.pinned) && (
            <ModeSwitch
              hacked={selection.hacked}
              compact
              onChange={(hacked) => {
                setError(null);
                launcher.select({ ...selection, hacked });
              }}
            />
          )}
        </div>
      </header>
      {error && <p className={styles.error}>{error}</p>}
      {vanilla && <p className={styles.banner}>Vanilla runs without mods. Pick Fabric or Forge to use some.</p>}
      {!vanilla && status && !status.installed && (
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
        {status?.extras.map((extra) => (
          <li key={extra.file} className={cx(styles.row, extra.hacks && styles.hackedRow)}>
            <div className={styles.what}>
              <span className={styles.name}>{extra.title}</span>
              <span className={styles.state}>{extra.file}</span>
            </div>
            <span className={cx(styles.badge, extra.hacks && styles.redBadge)}>{extra.hacks ? "Hacked only" : "Ours"}</span>
          </li>
        ))}
        {!vanilla && status && (
          <li className={styles.row}>
            <div className={styles.what}>
              <span className={styles.name}>CustomSkinLoader</span>
              <span className={cx(styles.state, status.skinNote && styles.skipped)}>
                {status.skinNote ?? status.skinMod ?? "Picked on the first Play"}
              </span>
            </div>
            <span className={styles.badge}>Shows your skin</span>
          </li>
        )}
      </ul>
      {!vanilla && <LoaderBuild launcher={launcher} />}
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
