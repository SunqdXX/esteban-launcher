import { DISCLAIMER, PAGE_LABELS, PAGES, type PageId } from "../pages";
import styles from "./Sidebar.module.css";

interface SidebarProps {
  current: PageId;
  onSelect: (page: PageId) => void;
}

export default function Sidebar({ current, onSelect }: SidebarProps) {
  return (
    <nav className={styles.sidebar} aria-label="Main">
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
      <footer className={styles.footer}>
        <p className={styles.disclaimer}>{DISCLAIMER}</p>
      </footer>
    </nav>
  );
}
