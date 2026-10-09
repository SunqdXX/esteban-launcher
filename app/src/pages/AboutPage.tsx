import { useEffect, useState } from "react";
import { api, message, onUpdateProgress, type About, type Coin, type ReleaseStatus, type UpdateCheck } from "../api";
import logoUrl from "../assets/logo/EstebanLogo.png";
import styles from "./AboutPage.module.css";

function Qr({ rows }: { rows: string[] }) {
  const size = rows.length;
  return (
    <svg className={styles.qr} viewBox={`-4 -4 ${String(size + 8)} ${String(size + 8)}`} shapeRendering="crispEdges" aria-hidden="true">
      <rect x="-4" y="-4" width={size + 8} height={size + 8} fill="#ffffff" />
      {rows.flatMap((row, y) =>
        row.split("").map((cell, x) => (cell === "1" ? <rect key={`${String(x)}-${String(y)}`} x={x} y={y} width="1" height="1" fill="#1a1b26" /> : null)),
      )}
    </svg>
  );
}

function CoinCard({ coin }: { coin: Coin }) {
  const [copied, setCopied] = useState<string | null>(null);
  return (
    <div className={styles.coin}>
      <div className={styles.coinHead}>
        <span className={styles.coinName}>{coin.name}</span>
        {coin.note && <span className={styles.coinNote}>{coin.note}</span>}
      </div>
      <Qr rows={coin.qr} />
      <input
        className={styles.address}
        readOnly
        value={coin.address}
        aria-label={`${coin.name} address`}
        onFocus={(e) => {
          e.target.select();
        }}
      />
      <button
        type="button"
        className={styles.copy}
        onClick={() => {
          navigator.clipboard
            .writeText(coin.address)
            .then(() => {
              setCopied("Copied");
            })
            .catch(() => {
              setCopied("Select the address and copy it by hand");
            });
        }}
      >
        {copied ?? "Copy address"}
      </button>
    </div>
  );
}

function updateLine(update: UpdateCheck): { text: string; good: boolean } {
  switch (update.kind) {
    case "noKey":
      return { text: "Updates aren't checked yet: no updater key is built into this launcher.", good: false };
    case "nothingPublished":
      return { text: "No launcher update has been published yet.", good: true };
    case "offline":
      return { text: "Couldn't reach GitHub to check for updates.", good: false };
    case "upToDate":
      return { text: `Up to date. The newest release is ${update.latest}.`, good: true };
    case "available":
      return { text: `Launcher ${update.version} is out, signature checked (key ${update.keyId}).${update.notes ? ` ${update.notes}` : ""}`, good: true };
    case "refused":
      return { text: update.reason, good: false };
  }
}

function channelLine(status: ReleaseStatus): string {
  const c = status.channel;
  const from =
    c.source === "github"
      ? `from GitHub, signed by key ${c.keyId ?? ""}`
      : c.source === "saved"
        ? `the last signed copy on this computer (key ${c.keyId ?? ""})`
        : "the list built into this launcher";
  return `Esteban version list ${String(c.sequence)}: ${from}. Expires ${c.expires.slice(0, 10)}.`;
}

type Phase =
  | { kind: "idle" }
  | { kind: "installing"; downloaded: number; total: number | null }
  | { kind: "installed"; version: string }
  | { kind: "failed"; text: string };

function megabytes(bytes: number): string {
  return (bytes / 1_048_576).toFixed(1);
}

function Updates() {
  const [status, setStatus] = useState<ReleaseStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(true);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  const load = () => {
    api
      .releaseStatus()
      .then((next) => {
        setStatus(next);
        setError(null);
      })
      .catch((e: unknown) => {
        setError(message(e));
      })
      .finally(() => {
        setChecking(false);
      });
  };

  const check = () => {
    setChecking(true);
    load();
  };

  useEffect(load, []);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    onUpdateProgress((p) => {
      setPhase((current) => (current.kind === "installing" ? { kind: "installing", downloaded: p.downloaded, total: p.total } : current));
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((e: unknown) => {
        setError(message(e));
      });
    return () => {
      cancelled = true;
      if (stop) stop();
    };
  }, []);

  const install = () => {
    setPhase({ kind: "installing", downloaded: 0, total: null });
    api
      .installUpdate()
      .then((done) => {
        setPhase({ kind: "installed", version: done.version });
      })
      .catch((e: unknown) => {
        setPhase({ kind: "failed", text: message(e) });
      });
  };

  const line = status ? updateLine(status.update) : null;
  const available = status?.update.kind === "available" ? status.update : null;
  return (
    <section className={styles.section}>
      <h2 className={styles.h2}>Updates</h2>
      <ul className={styles.facts}>
        <li>
          Esteban Launcher {status?.launcher ?? ""}.{" "}
          {checking && !status ? "Checking" : line ? <span className={line.good ? undefined : styles.dim}>{line.text}</span> : null}
        </li>
        {status && <li>{channelLine(status)}</li>}
        {status?.channel.notice && <li className={styles.bad}>{status.channel.notice}</li>}
      </ul>
      {error && <p className={styles.bad}>{error}</p>}
      {available && status?.selfUpdate.package && (
        <div className={styles.install}>
          {phase.kind === "installing" ? (
            <div role="status" aria-live="polite">
              <div className={styles.track}>
                <div
                  className={styles.fill}
                  style={{ width: phase.total ? `${String(Math.round((phase.downloaded / phase.total) * 100))}%` : "0%" }}
                />
              </div>
              <p className={styles.dim}>
                Downloading {megabytes(phase.downloaded)}
                {phase.total ? ` of ${megabytes(phase.total)}` : ""} MB, then checking its signature
              </p>
            </div>
          ) : phase.kind === "installed" ? (
            <>
              <p>{phase.version} is installed. Restart to use it.</p>
              <button
                type="button"
                className={styles.primary}
                onClick={() => {
                  api.restartApp().catch((e: unknown) => {
                    setError(message(e));
                  });
                }}
              >
                Restart now
              </button>
            </>
          ) : (
            <>
              <button type="button" className={styles.primary} onClick={install}>
                Download and install {available.version}
              </button>
              {phase.kind === "failed" && <p className={styles.bad}>{phase.text}</p>}
            </>
          )}
        </div>
      )}
      {available && status && !status.selfUpdate.package && (
        <div className={styles.install}>
          <p className={styles.dim}>{status.selfUpdate.reason}</p>
          <button
            type="button"
            className={styles.link}
            onClick={() => {
              api.openLink("releases").catch((e: unknown) => {
                setError(message(e));
              });
            }}
          >
            Open the release page
          </button>
        </div>
      )}
      <p className={styles.dim}>Updates and the Esteban version list are only used after their signatures check out against keys built into the launcher.</p>
      <button type="button" className={styles.link} disabled={checking || phase.kind === "installing"} onClick={check}>
        {checking ? "Checking" : "Check again"}
      </button>
    </section>
  );
}

export default function AboutPage() {
  const [about, setAbout] = useState<About | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .about()
      .then(setAbout)
      .catch((e: unknown) => {
        setError(message(e));
      });
  }, []);

  const open = (which: "github" | "esteban" | "discord") => {
    api.openLink(which).catch((e: unknown) => {
      setError(message(e));
    });
  };

  if (!about) return <div className={styles.page}>{error && <p className={styles.bad}>{error}</p>}</div>;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <img className={styles.logo} src={logoUrl} alt="Esteban" width={1408} height={768} />
        <div>
          <h1 className={styles.title}>Esteban Launcher</h1>
          <p className={styles.version}>version {about.version}</p>
          <p className={styles.disclaimer}>{about.disclaimer}</p>
        </div>
      </header>
      {error && <p className={styles.bad}>{error}</p>}

      <section className={styles.section}>
        <h2 className={styles.h2}>What it does</h2>
        <ul className={styles.facts}>
          <li>Installs the game, Java, Fabric and the mods from their official sources and checks every file against its published hash.</li>
          <li>Normal never gets the hacks mod. Hacked keeps it in its own folders.</li>
          <li>No ads, no tracking, no accounts of its own. Free, nothing is for sale.</li>
          <li>It only talks to Mojang, Fabric, Modrinth, GitHub for the Esteban jars, and Microsoft for sign-in once that's on.</li>
        </ul>
      </section>

      <section className={styles.section}>
        <h2 className={styles.h2}>Links</h2>
        <div className={styles.links}>
          <button
            type="button"
            className={styles.link}
            onClick={() => {
              open("github");
            }}
          >
            Launcher on GitHub
          </button>
          <button
            type="button"
            className={styles.link}
            onClick={() => {
              open("esteban");
            }}
          >
            Esteban mod on GitHub
          </button>
          <button
            type="button"
            className={styles.link}
            disabled={!about.discord}
            title={about.discord ? undefined : "The Discord server isn't open yet"}
            onClick={() => {
              open("discord");
            }}
          >
            {about.discord ? "Discord" : "Discord, soon"}
          </button>
        </div>
      </section>

      <Updates />

      {about.coins.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.h2}>Support</h2>
          <p className={styles.text}>Crypto only, and never needed for anything. Everything in the launcher stays free.</p>
          <div className={styles.coins}>
            {about.coins.map((coin) => (
              <CoinCard key={coin.name} coin={coin} />
            ))}
          </div>
        </section>
      )}

      <section className={styles.section}>
        <h2 className={styles.h2}>Licenses</h2>
        <ul className={styles.facts}>
          <li>The launcher is free software under the GPL-3.0.</li>
          <li>The logo and the screenshots are not covered by the GPL. Screenshots by SunqdXX using the IterationT shader pack by Tahnass, Minecraft content shown belongs to Mojang.</li>
          <li>IBM Plex Sans and Departure Mono are under the SIL Open Font License 1.1.</li>
          <li>Mods are downloaded unmodified from Modrinth and keep their own licenses.</li>
        </ul>
      </section>
    </div>
  );
}
