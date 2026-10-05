import { getCurrentWindow } from "@tauri-apps/api/window";
import { cx } from "../cx";
import PixelIcon, { CLOSE, MAXIMIZE, MINIMIZE } from "./PixelIcon";
import styles from "./Titlebar.module.css";

function run(action: () => Promise<void>) {
  action().catch((error: unknown) => {
    console.error(error);
  });
}

export default function Titlebar() {
  return (
    <header className={styles.bar}>
      <div className={styles.title} data-tauri-drag-region>
        esteban launcher
      </div>
      <div className={styles.controls}>
        <button
          type="button"
          className={styles.control}
          aria-label="Minimize"
          onClick={() => {
            run(() => getCurrentWindow().minimize());
          }}
        >
          <PixelIcon cells={MINIMIZE} />
        </button>
        <button
          type="button"
          className={styles.control}
          aria-label="Maximize"
          onClick={() => {
            run(() => getCurrentWindow().toggleMaximize());
          }}
        >
          <PixelIcon cells={MAXIMIZE} />
        </button>
        <button
          type="button"
          className={cx(styles.control, styles.close)}
          aria-label="Close"
          onClick={() => {
            run(() => getCurrentWindow().close());
          }}
        >
          <PixelIcon cells={CLOSE} />
        </button>
      </div>
    </header>
  );
}
