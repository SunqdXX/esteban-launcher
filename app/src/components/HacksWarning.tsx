import { useEffect, useRef } from "react";
import styles from "./HacksWarning.module.css";

interface HacksWarningProps {
  warning: string;
  onAccept: () => void;
  onCancel: () => void;
}

export default function HacksWarning({ warning, onAccept, onCancel }: HacksWarningProps) {
  const cancel = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancel.current?.focus();
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("keydown", escape);
    };
  }, [onCancel]);

  return (
    <div className={styles.backdrop}>
      <div className={styles.dialog} role="alertdialog" aria-modal="true" aria-labelledby="hacks-title" aria-describedby="hacks-text">
        <p className={styles.label}>Hacked mode</p>
        <h2 id="hacks-title" className={styles.title}>
          {warning}
        </h2>
        <div id="hacks-text" className={styles.text}>
          <p>Hacked adds the Esteban hacks mod on top of Normal, in its own folder. Normal never downloads it.</p>
          <p>Hacks start off every time the game starts. You only see this once.</p>
        </div>
        <div className={styles.actions}>
          <button ref={cancel} type="button" className={styles.cancel} onClick={onCancel}>
            Stay on Normal
          </button>
          <button type="button" className={styles.accept} onClick={onAccept}>
            I understand
          </button>
        </div>
      </div>
    </div>
  );
}
