import { useEffect, useState } from "react";
import { LOADER_LABEL, api, buildLabel, message, plural, sameSelection, type InstanceStatus, type Selection, type SkinCard } from "../api";
import PixelIcon, { FOLDER } from "../components/PixelIcon";
import SkinPreview, { Silhouette } from "../components/SkinPreview";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./ProfilesPage.module.css";

interface ProfilesPageProps {
  launcher: Launcher;
  onPlay: () => void;
}

const of = (s: InstanceStatus): Selection => ({ gameVersion: s.gameVersion, loader: s.loader, hacked: s.hacked });

export default function ProfilesPage({ launcher, onPlay }: ProfilesPageProps) {
  const { overview, selection, status, installing, revision, releases } = launcher;
  const [found, setFound] = useState<InstanceStatus[] | null>(null);
  const [skins, setSkins] = useState<SkinCard[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.instances(), api.skins()])
      .then(([list, library]) => {
        if (cancelled) return;
        setFound(list);
        setSkins(library);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(message(e));
      });
    return () => {
      cancelled = true;
    };
  }, [revision]);

  if (!overview || !selection) return null;

  const order = (s: InstanceStatus) => {
    const at = releases.findIndex((r) => r.id === s.gameVersion);
    return at < 0 ? releases.length : at;
  };
  const rows = [...(found ?? [])];
  if (status && !rows.some((r) => sameSelection(of(r), selection))) rows.push(status);
  rows.sort((a, b) => order(a) - order(b) || a.loader.localeCompare(b.loader) || Number(a.hacked) - Number(b.hacked));

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Profiles</h1>
        <p className={styles.sub}>Every version and loader keeps its own folder, so worlds, settings and mods stay apart.</p>
      </header>
      {error && <p className={styles.error}>{error}</p>}
      {found && rows.length === 0 && <p className={styles.empty}>Nothing installed yet. Pick a version on Play and hit PLAY.</p>}
      {rows.length > 0 && (
        <ul className={styles.rows}>
          <li className={cx(styles.row, styles.columns)} aria-hidden="true">
            <span>Loader</span>
            <span>Version</span>
            <span>Build</span>
            <span>Skin</span>
            <span />
            <span />
          </li>
          {rows.map((s) => {
            const target = of(s);
            const current = sameSelection(target, selection);
            const busy = sameSelection(installing, target);
            const mods = s.mods.filter((m) => m.installed).length + s.extras.length + (s.skinMod ? 1 : 0);
            const skin = skins.find((k) => k.id === s.skin);
            const tag = overview.pinned.find((p) => p.id === s.gameVersion)?.tag ?? "";
            let state: string;
            if (busy) state = "Installing";
            else if (!s.installed) state = "Not installed";
            else if (s.loader === "vanilla") state = "Vanilla, no mods";
            else state = `${LOADER_LABEL[s.loader]} ${buildLabel(s.gameVersion, s.loaderVersion)}${s.loaderPinned ? " (picked)" : ""} · ${plural(mods, "mod")}`;
            return (
              <li key={`${s.loader}-${s.gameVersion}-${String(s.hacked)}`} className={cx(styles.row, current && styles.current)}>
                <span className={styles.loader}>
                  <span className={cx(styles.dot, styles[s.hacked ? "hacked" : s.loader])} />
                  {LOADER_LABEL[s.loader]}
                  {s.hacked && <span className={styles.hackedTag}>Hacked</span>}
                </span>
                <span className={styles.v}>
                  {s.gameVersion}
                  {tag && <span className={styles.tag}>{tag}</span>}
                </span>
                <span className={styles.state}>{state}</span>
                <span className={styles.skin} title={s.skinNote ?? undefined}>
                  <span className={styles.head}>
                    {skin ? <SkinPreview rows={skin.preview} head label={skin.name} /> : <Silhouette head label="Account skin" />}
                  </span>
                  <span className={cx(styles.skinName, !skin && styles.dim)}>{s.skinNote && s.loader === "vanilla" ? "Account skin only" : (skin?.name ?? "Account skin")}</span>
                </span>
                <button
                  type="button"
                  className={styles.icon}
                  aria-label={`Open the ${LOADER_LABEL[s.loader]} ${s.gameVersion} folder`}
                  title="Open folder"
                  disabled={!s.installed}
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
      )}
    </div>
  );
}
