import { useState } from "react";
import styles from "./App.module.css";
import Placeholder from "./components/Placeholder";
import Sidebar from "./components/Sidebar";
import Titlebar from "./components/Titlebar";
import { PAGE_LABELS, type PageId } from "./pages";

export default function App() {
  const [page, setPage] = useState<PageId>("play");

  return (
    <div className={styles.shell}>
      <Titlebar />
      <div className={styles.body}>
        <Sidebar current={page} onSelect={setPage} />
        <main className={styles.main}>
          <Placeholder title={PAGE_LABELS[page]} showLogo={page === "play"} />
        </main>
      </div>
    </div>
  );
}
