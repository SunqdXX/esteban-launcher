import { LOADER_LABEL, LOADERS, offerFor, type Loader, type Release } from "../api";
import { cx } from "../cx";
import styles from "./ModeSwitch.module.css";

interface LoaderSwitchProps {
  release: Release | undefined;
  value: Loader;
  disabled?: boolean;
  compact?: boolean;
  onChange: (loader: Loader) => void;
}

export default function LoaderSwitch({ release, value, disabled, compact, onChange }: LoaderSwitchProps) {
  return (
    <div className={cx(styles.modes, styles.three, compact && styles.compact)} role="radiogroup" aria-label="Loader">
      {LOADERS.map((loader) => {
        const offer = offerFor(release, loader);
        return (
          <button
            key={loader}
            type="button"
            role="radio"
            aria-checked={loader === value}
            disabled={disabled === true || !offer.available}
            title={offer.available ? undefined : (offer.reason ?? undefined)}
            className={cx(styles.mode, loader === value && styles.on, !offer.available && styles.unavailable)}
            onClick={() => {
              if (loader !== value) onChange(loader);
            }}
          >
            <span className={styles.dot} />
            {LOADER_LABEL[loader]}
          </button>
        );
      })}
    </div>
  );
}
