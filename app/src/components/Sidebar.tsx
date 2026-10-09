import logoUrl from "../assets/logo/EstebanLogo.png";
import { DISCLAIMER, PAGE_LABELS, PAGES, type PageId } from "../pages";
import styles from "./Sidebar.module.css";

interface SidebarProps {
  current: PageId;
  update: string | null;
  onSelect: (page: PageId) => void;
}

export default function Sidebar({ current, update, onSelect }: SidebarProps) {
  return (
    <nav className={styles.sidebar} aria-label="Main">
      <div className={styles.brand}>
        <img src={logoUrl} alt="Esteban" width={1408} height={768} />
      </div>
      <ul className={styles.nav}>
        {PAGES.map((page) => (
          <li key={page}>
            <button
              type="button"
              className={styles.item}
              aria-current={page === current ? "page" : undefined}
              onClick={() => {
                onSelect(page);
              }}
            >
              {PAGE_LABELS[page]}
            </button>
          </li>
        ))}
      </ul>
      {update && (
        <button
          type="button"
          className={styles.update}
          onClick={() => {
            onSelect("about");
          }}
        >
          <span className={styles.updateDot} />
          Launcher {update} is out
        </button>
      )}
      <footer className={styles.footer}>
        <p className={styles.disclaimer}>{DISCLAIMER}</p>
      </footer>
    </nav>
  );
}
