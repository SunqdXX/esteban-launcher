import { useEffect, useState } from "react";
import { api, message, selectionLabel, type Model, type SkinCard } from "../api";
import SkinPreview, { Silhouette } from "../components/SkinPreview";
import { cx } from "../cx";
import type { Launcher } from "../useLauncher";
import styles from "./SkinsPage.module.css";

interface SkinsPageProps {
  launcher: Launcher;
}

const MODELS: { model: Model; label: string }[] = [
  { model: "classic", label: "Classic" },
  { model: "slim", label: "Slim" },
];

export default function SkinsPage({ launcher }: SkinsPageProps) {
  const { selection, status } = launcher;
  const [skins, setSkins] = useState<SkinCard[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  useEffect(() => {
    api
      .skins()
      .then(setSkins)
      .catch((e: unknown) => {
        setError(message(e));
      });
  }, []);

  useEffect(() => {
    if (!confirm) return;
    const timer = window.setTimeout(() => {
      setConfirm(null);
    }, 3000);
    return () => {
      window.clearTimeout(timer);
    };
  }, [confirm]);

  if (!selection) return null;
  const blocked = status?.skinNote ?? null;
  const target = selectionLabel(selection);
  const active = status?.skin ?? null;

  const replace = (next: SkinCard) => {
    setSkins((list) => (list ?? []).map((s) => (s.id === next.id ? next : s)));
  };

  const use = (id: string | null) => {
    setError(null);
    api
      .setSkin(selection, id)
      .then(launcher.setStatus)
      .catch((e: unknown) => {
        setError(message(e));
      });
  };

  const skinButton = (id: string | null) => {
    const on = active === id || (id === null && !skins?.some((s) => s.id === active));
    return (
      <button
        type="button"
        className={cx(styles.use, on && styles.inUse)}
        disabled={on || (id !== null && !!blocked)}
        onClick={() => {
          use(id);
        }}
      >
        {on ? `On ${target}` : `Use on ${target}`}
      </button>
    );
  };

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div>
          <h1 className={styles.title}>Skins</h1>
          <p className={styles.sub}>
            Kept on this computer and shown only to you, through the skin mod in Fabric and Forge instances. No sign-in needed.
          </p>
        </div>
        <button
          type="button"
          className={styles.add}
          onClick={() => {
            setError(null);
            api
              .importSkin()
              .then((added) => {
                if (added) setSkins((list) => [...(list ?? []).filter((s) => s.id !== added.id), added]);
              })
              .catch((e: unknown) => {
                setError(message(e));
              });
          }}
        >
          Add a skin
        </button>
      </header>
      <p className={cx(styles.target, blocked && styles.blocked)}>
        {blocked ?? `Picking for ${target}. It shows from the next launch.`}
      </p>
      {error && <p className={styles.error}>{error}</p>}
      <ul className={styles.grid}>
        <li className={styles.card}>
          <div className={styles.preview}>
            <Silhouette label="Your account skin" />
          </div>
          <p className={styles.name}>Account skin</p>
          <p className={styles.note}>The skin on your Minecraft account, nothing local.</p>
          {skinButton(null)}
        </li>
        {skins?.map((skin) => (
          <li key={skin.id} className={cx(styles.card, active === skin.id && styles.current)}>
            <div className={styles.preview}>
              <SkinPreview rows={skin.preview} label={skin.name} />
            </div>
            <input
              className={styles.rename}
              defaultValue={skin.name}
              aria-label="Skin name"
              maxLength={40}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
              }}
              onBlur={(e) => {
                const name = e.currentTarget.value.trim();
                if (name === skin.name) return;
                api
                  .updateSkin(skin.id, { name })
                  .then(replace)
                  .catch((err: unknown) => {
                    setError(message(err));
                  });
              }}
            />
            <div className={styles.models} role="radiogroup" aria-label={`${skin.name} arms`}>
              {MODELS.map((m) => (
                <button
                  key={m.model}
                  type="button"
                  role="radio"
                  aria-checked={skin.model === m.model}
                  className={cx(styles.model, skin.model === m.model && styles.modelOn)}
                  onClick={() => {
                    if (skin.model === m.model) return;
                    api
                      .updateSkin(skin.id, { model: m.model })
                      .then(replace)
                      .catch((err: unknown) => {
                        setError(message(err));
                      });
                  }}
                >
                  {m.label}
                </button>
              ))}
            </div>
            {skinButton(skin.id)}
            <button
              type="button"
              className={cx(styles.remove, confirm === skin.id && styles.sure)}
              onClick={() => {
                if (confirm !== skin.id) {
                  setConfirm(skin.id);
                  return;
                }
                setConfirm(null);
                api
                  .removeSkin(skin.id)
                  .then(() => {
                    setSkins((list) => (list ?? []).filter((s) => s.id !== skin.id));
                    launcher.refreshStatus();
                  })
                  .catch((err: unknown) => {
                    setError(message(err));
                  });
              }}
            >
              {confirm === skin.id ? "Click again to remove" : "Remove"}
            </button>
          </li>
        ))}
      </ul>
      {skins?.length === 0 && <p className={styles.hint}>Add a 64x64 PNG skin to get started. The old 64x32 ones work too.</p>}
      <section className={styles.account}>
        <h2 className={styles.h2}>Your Minecraft account</h2>
        <p className={styles.hint}>Put a skin on the account itself, so every player sees it on every server.</p>
        <button type="button" className={styles.upload} disabled>
          Upload to my Minecraft account
        </button>
        <p className={styles.hint}>Needs Microsoft sign-in, coming in a later update.</p>
      </section>
    </div>
  );
}
