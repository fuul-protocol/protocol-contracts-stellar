import { rpc } from "@stellar/stellar-sdk";

/** Local qualification observed this specific captive Core read error.
 * Retry only that structured error, at most twice. Keep all contract results and other errors unchanged.
 */
export async function captiveCoreRead<T>(read: () => Promise<T>, wait: (ms: number) => Promise<void> = ms => new Promise(resolve => setTimeout(resolve, ms))): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try { return await read(); }
    catch (error) {
      const value = error as { code?: unknown; message?: unknown } | null;
      if (attempt >= 2 || value?.code !== -32603 || value.message !== "could not query captive core: http request failed with non-200 status code (404)") throw error;
      console.warn(JSON.stringify({ stage: "local-rpc-read-retry", attempt: attempt + 1, reason: "captive-core-404" }));
      await wait(250 * (attempt + 1));
    }
  }
}

/** Local test transport. Submission is inherited unchanged and is never retried here. */
export class LocalRpcServer extends rpc.Server {
  constructor(url: string) {
    super(url, { allowHttp: true });
    Object.assign(this.httpClient.defaults, { timeout: 15_000, maxRedirects: 0, maxContentLength: 64 * 1024 * 1024 });
  }
  override getHealth(...args: Parameters<rpc.Server["getHealth"]>) { return captiveCoreRead(() => super.getHealth(...args)); }
  override getNetwork(...args: Parameters<rpc.Server["getNetwork"]>) { return captiveCoreRead(() => super.getNetwork(...args)); }
  override getLatestLedger(...args: Parameters<rpc.Server["getLatestLedger"]>) { return captiveCoreRead(() => super.getLatestLedger(...args)); }
  override getLedgerEntries(...args: Parameters<rpc.Server["getLedgerEntries"]>) { return captiveCoreRead(() => super.getLedgerEntries(...args)); }
  override getLedgerEntry(...args: Parameters<rpc.Server["getLedgerEntry"]>) { return captiveCoreRead(() => super.getLedgerEntry(...args)); }
  override getTransaction(...args: Parameters<rpc.Server["getTransaction"]>) { return captiveCoreRead(() => super.getTransaction(...args)); }
  override simulateTransaction(...args: Parameters<rpc.Server["simulateTransaction"]>) { return captiveCoreRead(() => super.simulateTransaction(...args)); }
}
