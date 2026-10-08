import { useEffect, useState } from "react";
import { api, message, type About, type Coin } from "../api";
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
  const empty = coin.address.length === 0;
  return (
    <div className={styles.coin}>
      <div className={styles.coinHead}>
        <span className={styles.coinName}>{coin.name}</span>
        {coin.note && <span className={styles.coinNote}>{coin.note}</span>}
      </div>
      {empty ? (
        <p className={styles.empty}>Address not set yet</p>
      ) : (
        <>
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
        </>
      )}
    </div>
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

      <section className={styles.section}>
        <h2 className={styles.h2}>Support</h2>
        <p className={styles.text}>Crypto only, and never needed for anything. Everything in the launcher stays free.</p>
        <div className={styles.coins}>
          {about.coins.map((coin) => (
            <CoinCard key={coin.name} coin={coin} />
          ))}
        </div>
      </section>

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
