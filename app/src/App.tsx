import { useState } from "react";
import styles from "./App.module.css";
import Placeholder from "./components/Placeholder";
import Sidebar from "./components/Sidebar";
import Titlebar from "./components/Titlebar";
import ModsPage from "./pages/ModsPage";
import PlayPage from "./pages/PlayPage";
import ProfilesPage from "./pages/ProfilesPage";
import { PAGE_LABELS, type PageId } from "./pages";
import { useLauncher } from "./useLauncher";

export default function App() {
  const [page, setPage] = useState<PageId>("play");
  const launcher = useLauncher();

  let content;
  if (launcher.loadError && !launcher.overview) {
    content = (
      <section className={styles.failed}>
        <h1>The launcher couldn't read its settings</h1>
        <p>{launcher.loadError}</p>
      </section>
    );
  } else if (page === "play") {
    content = <PlayPage launcher={launcher} />;
  } else if (page === "profiles") {
    content = (
      <ProfilesPage
        launcher={launcher}
        onPlay={() => {
          setPage("play");
        }}
      />
    );
  } else if (page === "mods") {
    content = <ModsPage launcher={launcher} />;
  } else {
    content = <Placeholder title={PAGE_LABELS[page]} />;
  }

  return (
    <div className={styles.shell}>
      <Titlebar />
      <div className={styles.body}>
        <Sidebar current={page} onSelect={setPage} />
        <main className={styles.main}>{content}</main>
      </div>
    </div>
  );
}
