import { useState } from "react";
import { MODE_LABEL, listTitles, type InstanceStatus, type Progress, type Selection } from "../api";
import letters from "../assets/backgrounds/letters.png";
import meadow from "../assets/backgrounds/meadow.png";
import AccountCard from "../components/AccountCard";
import HacksWarning from "../components/HacksWarning";
import ModeSwitch from "../components/ModeSwitch";
import VersionPicker from "../components/VersionPicker";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./PlayPage.module.css";

const STAGES: Record<string, string> = {
  game: "Game files",
  libraries: "Libraries",
  "asset index": "Asset index",
  assets: "Sounds and textures",
  "java runtime": "Java",
  mods: "Mods",
};

function mb(bytes: number): string {
  return (bytes / 1_048_576).toFixed(1);
}

function cleanNote(status: InstanceStatus | null): string {
  if (!status) return "";
  const titles = status.mods
    .filter((m) => m.managed && m.wanted && !m.required && !(m.skipped && !m.installed))
    .map((m) => m.title);
  const fabric = status.loaderVersion ? `Fabric ${status.loaderVersion}` : "Fabric";
  return `${fabric} with ${listTitles(titles)}.`;
}

function progressLine(p: Progress, rate: number | null): string {
  const stage = STAGES[p.stage] ?? p.stage;
  const parts = [stage];
  if (p.files > 0) parts.push(`${String(p.filesDone)}/${String(p.files)} files`);
  if (p.bytes > 0) parts.push(`${mb(p.bytesDone)}/${mb(p.bytes)} MB`);
  if (rate && p.bytes > p.bytesDone) {
    const left = Math.max(1, Math.round((p.bytes - p.bytesDone) / rate));
    parts.push(left >= 60 ? `${String(Math.round(left / 60))} min left` : `${String(left)} s left`);
  }
  return parts.join(" · ");
}

function fraction(p: Progress): number {
  if (p.bytes > 0) return Math.min(1, p.bytesDone / p.bytes);
  if (p.files > 0) return Math.min(1, p.filesDone / p.files);
  return 0;
}

interface PlayPageProps {
  launcher: Launcher;
}

export default function PlayPage({ launcher }: PlayPageProps) {
  const { overview, selection, status, installing, progress, rate, outcome, notice } = launcher;
  const [warning, setWarning] = useState(false);
  if (!overview || !selection) return <div className={styles.play} />;

  const hacked = selection.profile === "hacks";
  const busy = installing !== null;
  const mine = installing?.gameVersion === selection.gameVersion && installing.profile === selection.profile;
  const pending = status?.mods.some((m) => m.managed && m.wanted !== m.installed && !(m.wanted && m.skipped)) ?? false;

  const start = (target: Selection) => {
    void launcher.install(target);
  };

  const play = () => {
    if (hacked && !overview.hacksWarningAccepted) {
      setWarning(true);
      return;
    }
    start(selection);
  };

  let line: { text: string; tone: "ok" | "info" | "error" | "dim" };
  if (outcome?.kind === "error") line = { text: outcome.text, tone: "error" };
  else if (outcome?.kind === "ready") line = { text: "Ready. Launching needs Microsoft sign-in, coming in a later update.", tone: "info" };
  else if (busy && !mine) line = { text: `Installing ${installing.gameVersion} ${MODE_LABEL[installing.profile]} first`, tone: "dim" };
  else if (!status) line = { text: "Checking", tone: "dim" };
  else if (!status.installed) line = { text: "Not installed yet", tone: "dim" };
  else if (pending) line = { text: "Mod changes apply on Play", tone: "info" };
  else line = { text: "Installed", tone: "ok" };

  return (
    <>
      <div className={styles.play}>
        <div className={cx(styles.wall, !hacked && styles.shown)} style={{ backgroundImage: `url(${meadow})` }} />
        <div className={cx(styles.wall, hacked && styles.shown)} style={{ backgroundImage: `url(${letters})` }} />
        <div className={styles.shade} />
        <div className={styles.top}>
          <AccountCard />
        </div>
        <div />
        <section className={styles.launch}>
          <div>
            <p className={styles.label}>Version</p>
            <VersionPicker
              versions={overview.versions}
              value={selection.gameVersion}
              disabled={busy}
              opensUp
              onChange={(gameVersion) => {
                launcher.select({ ...selection, gameVersion });
              }}
            />
          </div>
          <div>
            <p className={styles.label}>Mode</p>
            <ModeSwitch
              value={selection.profile}
              disabled={busy}
              onChange={(profile) => {
                launcher.select({ ...selection, profile });
              }}
            />
            {hacked ? (
              <p className={cx(styles.note, styles.warn)}>
                <b>Hacks mod on top of Normal.</b> Most servers ban it.{" "}
                {overview.hacksWarningAccepted ? "Hacks start off every launch." : "A one-time warning shows before the first launch."}
              </p>
            ) : (
              <p className={styles.note}>
                <b>{cleanNote(status)}</b> No cheat code.{status && !status.installed ? " Everything downloads and gets checked on the first Play." : ""}
              </p>
            )}
          </div>
          <div>
            <p className={styles.label}>&nbsp;</p>
            <button type="button" className={styles.go} disabled={busy} onClick={play}>
              <span className={styles.word}>PLAY</span>
              <span className={styles.what}>
                {selection.gameVersion} {MODE_LABEL[selection.profile]}
              </span>
            </button>
            <div className={styles.statusArea}>
              {mine && progress ? (
                <div className={styles.progress} role="status" aria-live="polite">
                  <div className={styles.track}>
                    <div className={styles.fill} style={{ width: `${String(Math.round(fraction(progress) * 100))}%` }} />
                  </div>
                  <p className={styles.status}>{progressLine(progress, rate)}</p>
                </div>
              ) : mine ? (
                <p className={styles.status} role="status">
                  <span className={cx(styles.sq, styles.dim)} />
                  Starting
                </p>
              ) : (
                <p className={cx(styles.status, styles[line.tone])} role="status">
                  <span className={styles.sq} />
                  {line.text}
                </p>
              )}
              {mine && notice && <p className={styles.notice}>{notice}</p>}
            </div>
          </div>
        </section>
      </div>
      {warning && (
        <HacksWarning
          warning={overview.hacksWarning}
          onCancel={() => {
            setWarning(false);
            launcher.select({ ...selection, profile: "clean" });
          }}
          onAccept={() => {
            setWarning(false);
            void launcher.acceptHacks().then((ok) => {
              if (ok) start(selection);
            });
          }}
        />
      )}
    </>
  );
}
