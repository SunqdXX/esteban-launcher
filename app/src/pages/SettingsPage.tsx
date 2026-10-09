import { useEffect, useRef, useState } from "react";
import { api, gigabytes, hackedAllowed, message, settle, type LauncherSettings, type PackLine, type Selection } from "../api";
import LoaderSwitch from "../components/LoaderSwitch";
import ModeSwitch from "../components/ModeSwitch";
import Toggle from "../components/Toggle";
import VersionPicker from "../components/VersionPicker";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./SettingsPage.module.css";

interface SettingsPageProps {
  launcher: Launcher;
}

type Note = { tone: "ok" | "error"; text: string } | null;

function Feedback({ note }: { note: Note }) {
  if (!note) return null;
  return <p className={cx(styles.feedback, note.tone === "error" && styles.bad)}>{note.text}</p>;
}

export default function SettingsPage({ launcher }: SettingsPageProps) {
  const { overview, selection, installing } = launcher;
  const [settings, setSettings] = useState<LauncherSettings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [memory, setMemory] = useState<number | null>(null);
  const [memoryNote, setMemoryNote] = useState<Note>(null);
  const [javaNote, setJavaNote] = useState<Note>(null);
  const [flags, setFlags] = useState("");
  const [flagsNote, setFlagsNote] = useState<Note>(null);
  const [folderNote, setFolderNote] = useState<Note>(null);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState<Selection | null>(selection);
  const [packLines, setPackLines] = useState<PackLine[]>([]);
  const [packNote, setPackNote] = useState<Note>(null);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    api
      .settings()
      .then((s) => {
        setSettings(s);
        setMemory(s.memoryMb);
        setFlags(s.jvmArgs);
        setSource(s.packSources[0] ?? "");
      })
      .catch((e: unknown) => {
        setLoadError(message(e));
      });
  }, []);

  if (loadError) return <div className={styles.page}><p className={styles.bad}>{loadError}</p></div>;
  if (!settings || !overview || !selection) return <div className={styles.page} />;

  const auto = memory === null;
  const shownMemory = memory ?? settings.autoMemoryMb;

  const saveMemory = (value: number | null) => {
    setMemory(value);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      api
        .setMemory(value)
        .then(() => {
          setMemoryNote({ tone: "ok", text: "Saved. Used on the next launch." });
        })
        .catch((e: unknown) => {
          setMemoryNote({ tone: "error", text: message(e) });
        });
    }, 350);
  };

  const applyJava = (path: string | null) => {
    setJavaNote(null);
    api
      .setJava(path)
      .then((version) => {
        setSettings({ ...settings, javaPath: path });
        setJavaNote({ tone: "ok", text: version ? `Found ${version}` : "Back to the launcher's own Java." });
      })
      .catch((e: unknown) => {
        setJavaNote({ tone: "error", text: message(e) });
      });
  };

  const applyFolder = (path: string | null) => {
    setFolderNote(null);
    api
      .setDataDir(path)
      .then((shown) => {
        setSettings({ ...settings, dataDir: shown });
        setFolderNote({ tone: "ok", text: "Saved. New installs and games go here now." });
        launcher.select(selection);
      })
      .catch((e: unknown) => {
        setFolderNote({ tone: "error", text: message(e) });
      });
  };

  const runPacks = (action: "link" | "import") => {
    if (!target || !source) return;
    setPackLines([]);
    setPackNote(null);
    api
      .packs(action, source, target)
      .then((lines) => {
        setPackLines(lines);
      })
      .catch((e: unknown) => {
        setPackNote({ tone: "error", text: message(e) });
      });
  };

  const packTarget = target ?? selection;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Settings</h1>
        <p className={styles.sub}>Everything here saves right away.</p>
      </header>

      <section className={styles.section}>
        <h2 className={styles.h2}>Account</h2>
        <div className={styles.account}>
          <div>
            <p className={styles.strong}>Not signed in</p>
            <p className={styles.text}>
              Sign-in uses your Microsoft account, the official way, and you need to own the game. It turns on once
              Mojang approves this launcher. No cracked or offline accounts.
            </p>
          </div>
          <button type="button" className={styles.button} disabled>
            Sign in with Microsoft
          </button>
        </div>
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Memory</h2>
        <div className={styles.line}>
          <Toggle
            on={auto}
            label="Pick memory automatically"
            onChange={(on) => {
              saveMemory(on ? null : settings.autoMemoryMb);
            }}
          />
          <span className={styles.strong}>Automatic</span>
          <span className={styles.text}>
            {gigabytes(settings.autoMemoryMb)} for this PC ({gigabytes(settings.totalMemoryMb)} installed)
          </span>
        </div>
        <div className={cx(styles.line, auto && styles.disabled)}>
          <input
            className={styles.range}
            type="range"
            aria-label="Memory for Minecraft"
            min={settings.minMemoryMb}
            max={settings.maxMemoryMb}
            step={256}
            value={shownMemory}
            disabled={auto}
            onChange={(e) => {
              saveMemory(Number(e.target.value));
            }}
          />
          <span className={styles.value}>{gigabytes(shownMemory)}</span>
        </div>
        <p className={styles.hint}>More isn't faster. 4 to 8 GB suits most setups, shaders like a bit more.</p>
        <Feedback note={memoryNote} />
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Java</h2>
        <p className={styles.text}>
          {settings.javaPath ? (
            <>
              Using <span className={styles.mono}>{settings.javaPath}</span>
            </>
          ) : (
            "Using the launcher's own Java, the right version for each Minecraft version, downloaded from Mojang. Recommended."
          )}
        </p>
        <div className={styles.buttons}>
          <button
            type="button"
            className={styles.button}
            onClick={() => {
              api
                .pickJava()
                .then((path) => {
                  if (path) applyJava(path);
                })
                .catch((e: unknown) => {
                  setJavaNote({ tone: "error", text: message(e) });
                });
            }}
          >
            Pick a java file
          </button>
          {settings.javaPath && (
            <button
              type="button"
              className={styles.button}
              onClick={() => {
                applyJava(null);
              }}
            >
              Use the launcher's Java
            </button>
          )}
        </div>
        <Feedback note={javaNote} />
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Extra JVM flags</h2>
        <p className={styles.text}>Added after the launcher's own flags. Leave empty unless you know what they do.</p>
        <form
          className={styles.buttons}
          onSubmit={(e) => {
            e.preventDefault();
            setFlagsNote(null);
            api
              .setJvmArgs(flags)
              .then((saved) => {
                setFlags(saved);
                setFlagsNote({ tone: "ok", text: saved ? "Saved." : "Saved, no extra flags." });
              })
              .catch((err: unknown) => {
                setFlagsNote({ tone: "error", text: message(err) });
              });
          }}
        >
          <input
            className={styles.input}
            value={flags}
            spellCheck={false}
            placeholder="-XX:+UseZGC"
            aria-label="Extra JVM flags"
            onChange={(e) => {
              setFlags(e.target.value);
            }}
          />
          <button type="submit" className={styles.button}>
            Save
          </button>
        </form>
        <Feedback note={flagsNote} />
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Game folder</h2>
        <p className={styles.text}>
          Games, Java and downloads live in <span className={styles.mono}>{settings.dataDir}</span>
        </p>
        <div className={styles.buttons}>
          <button
            type="button"
            className={styles.button}
            disabled={installing !== null}
            onClick={() => {
              api
                .pickFolder()
                .then((path) => {
                  if (path) applyFolder(path);
                })
                .catch((e: unknown) => {
                  setFolderNote({ tone: "error", text: message(e) });
                });
            }}
          >
            Change folder
          </button>
          {settings.dataDir !== settings.defaultDataDir && (
            <button
              type="button"
              className={styles.button}
              disabled={installing !== null}
              onClick={() => {
                applyFolder(null);
              }}
            >
              Use the default
            </button>
          )}
        </div>
        <p className={styles.hint}>Nothing gets moved. What's in the old folder stays there, Play installs again in the new one.</p>
        <Feedback note={folderNote} />
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Packs from another game folder</h2>
        <p className={styles.text}>Use the shader packs, resource packs and screenshots you already have in another launcher.</p>
        <div className={styles.buttons}>
          {settings.packSources.map((path) => (
            <button
              key={path}
              type="button"
              className={cx(styles.chip, source === path && styles.chipOn)}
              onClick={() => {
                setSource(path);
              }}
            >
              {path}
            </button>
          ))}
          <button
            type="button"
            className={styles.button}
            onClick={() => {
              api
                .pickFolder()
                .then((path) => {
                  if (path) setSource(path);
                })
                .catch((e: unknown) => {
                  setPackNote({ tone: "error", text: message(e) });
                });
            }}
          >
            Pick a folder
          </button>
        </div>
        <p className={styles.text}>
          From <span className={styles.mono}>{source || "no folder picked yet"}</span>
        </p>
        <div className={styles.target}>
          <VersionPicker
            releases={launcher.releases}
            pinned={overview.pinned}
            value={packTarget.gameVersion}
            compact
            onChange={(gameVersion) => {
              setTarget(settle({ ...packTarget, gameVersion }, launcher.releases, overview.pinned));
            }}
          />
          <LoaderSwitch
            release={launcher.releases.find((r) => r.id === packTarget.gameVersion)}
            value={packTarget.loader}
            compact
            onChange={(loader) => {
              setTarget(settle({ ...packTarget, loader }, launcher.releases, overview.pinned));
            }}
          />
          {hackedAllowed(packTarget, overview.pinned) && (
            <ModeSwitch
              hacked={packTarget.hacked}
              compact
              onChange={(hacked) => {
                setTarget({ ...packTarget, hacked });
              }}
            />
          )}
        </div>
        <div className={styles.buttons}>
          <button
            type="button"
            className={styles.button}
            disabled={!source}
            onClick={() => {
              runPacks("link");
            }}
          >
            Link
          </button>
          <button
            type="button"
            className={styles.button}
            disabled={!source}
            onClick={() => {
              runPacks("import");
            }}
          >
            Copy
          </button>
        </div>
        <p className={styles.hint}>Link shares the folders, so packs you add show up in both. Copy makes a one time copy and never overwrites your files.</p>
        {packLines.length > 0 && (
          <ul className={styles.results}>
            {packLines.map((line) => (
              <li key={line.folder}>
                <span className={styles.mono}>{line.folder}</span> {line.text}
              </li>
            ))}
          </ul>
        )}
        <Feedback note={packNote} />
      </section>
    </div>
  );
}
