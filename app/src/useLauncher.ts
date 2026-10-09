import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  message,
  onProgress,
  settle,
  type Catalog,
  type InstallSummary,
  type InstanceStatus,
  type Overview,
  type Progress,
  type Release,
  type Selection,
} from "./api";

export type Outcome = { kind: "ready"; summary: InstallSummary } | { kind: "error"; text: string } | null;

export interface Launcher {
  overview: Overview | null;
  catalog: Catalog | null;
  releases: Release[];
  loadError: string | null;
  selection: Selection | null;
  status: InstanceStatus | null;
  installing: Selection | null;
  progress: Progress | null;
  rate: number | null;
  outcome: Outcome;
  notice: string | null;
  revision: number;
  select: (next: Selection) => void;
  acceptHacks: () => Promise<boolean>;
  install: (target: Selection) => Promise<void>;
  setMod: (slug: string, enabled: boolean) => Promise<string | null>;
  setStatus: (next: InstanceStatus) => void;
  refreshStatus: () => void;
}

export function useLauncher(): Launcher {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [status, setStatus] = useState<InstanceStatus | null>(null);
  const [installing, setInstalling] = useState<Selection | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [rate, setRate] = useState<number | null>(null);
  const [outcome, setOutcome] = useState<Outcome>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const request = useRef(0);
  const stageStart = useRef<{ stage: string; at: number } | null>(null);

  const loadStatus = useCallback((target: Selection) => {
    const id = ++request.current;
    api
      .status(target)
      .then((next) => {
        if (id === request.current) setStatus(next);
      })
      .catch((error: unknown) => {
        if (id === request.current) {
          setStatus(null);
          setLoadError(message(error));
        }
      });
  }, []);

  useEffect(() => {
    api
      .overview()
      .then((next) => {
        setOverview(next);
        const first = { gameVersion: next.gameVersion, loader: next.loader, hacked: next.hacked };
        setSelection(first);
        loadStatus(first);
      })
      .catch((error: unknown) => {
        setLoadError(message(error));
      });
    api
      .catalog(false)
      .then(setCatalog)
      .catch((error: unknown) => {
        setCatalog({ latest: "", releases: [], offline: true });
        setLoadError(message(error));
      });
  }, [loadStatus]);

  const releases = catalog?.releases ?? [];

  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    onProgress((p) => {
      setProgress(p);
      const now = performance.now();
      if (p.notice) setNotice(p.notice);
      else if (stageStart.current?.stage !== p.stage) setNotice(null);
      if (stageStart.current?.stage !== p.stage) {
        stageStart.current = { stage: p.stage, at: now };
        setRate(null);
        return;
      }
      const seconds = (now - stageStart.current.at) / 1000;
      if (seconds > 1 && p.bytesDone > 0) setRate(p.bytesDone / seconds);
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((error: unknown) => {
        setLoadError(message(error));
      });
    return () => {
      cancelled = true;
      if (stop) stop();
    };
  }, []);

  const select = useCallback(
    (wanted: Selection) => {
      const next = settle(wanted, catalog?.releases ?? [], overview?.pinned ?? []);
      setSelection(next);
      setOutcome(null);
      setStatus(null);
      loadStatus(next);
      api.select(next).catch((error: unknown) => {
        setOutcome({ kind: "error", text: message(error) });
      });
    },
    [loadStatus, catalog, overview],
  );

  const refreshStatus = useCallback(() => {
    if (selection) loadStatus(selection);
  }, [loadStatus, selection]);

  const acceptHacks = useCallback(async () => {
    try {
      await api.acceptHacksWarning();
      setOverview((current) => (current ? { ...current, hacksWarningAccepted: true } : current));
      return true;
    } catch (error) {
      setOutcome({ kind: "error", text: message(error) });
      return false;
    }
  }, []);

  const install = useCallback(
    async (target: Selection) => {
      setInstalling(target);
      setOutcome(null);
      setNotice(null);
      setProgress(null);
      setRate(null);
      stageStart.current = null;
      try {
        const summary = await api.install(target);
        setOutcome({ kind: "ready", summary });
      } catch (error) {
        setOutcome({ kind: "error", text: message(error) });
      } finally {
        setInstalling(null);
        setProgress(null);
        setRevision((r) => r + 1);
        if (selection) loadStatus(selection);
      }
    },
    [loadStatus, selection],
  );

  const setMod = useCallback(
    async (slug: string, enabled: boolean) => {
      if (!selection) return null;
      try {
        setStatus(await api.setMod(selection, slug, enabled));
        return null;
      } catch (error) {
        return message(error);
      }
    },
    [selection],
  );

  return {
    overview,
    catalog,
    releases,
    loadError,
    selection,
    status,
    installing,
    progress,
    rate,
    outcome,
    notice,
    revision,
    select,
    acceptHacks,
    install,
    setMod,
    setStatus,
    refreshStatus,
  };
}
