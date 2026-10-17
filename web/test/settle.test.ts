// settle: answers that land late are applied only if no Change Notice came
// in between, and every request's AbortSignal is aborted exactly when its
// answer is no longer wanted.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { expect, test } from "vitest";
import { createSettler } from "../src/settle";

/** A request the test answers when it chooses, as a slow source would. */
interface Asked {
  /** Stamped with the epoch it was asked in (Windows, cells, counts), or not (sorts, Locates). */
  fresh: boolean;
  epoch: number;
  signal: AbortSignal;
  resolve(value: number): void;
  reject(error: unknown): void;
  /** Whether the source has answered yet (it may answer even after an abort). */
  answered: boolean;
  /**
   * Whether the answer should apply when it lands: not aborted, and for a
   * fresh request no Change Notice since it was asked. `null` until it lands.
   */
  wanted: boolean | null;
  /** The source rejected it. */
  failed: boolean;
  /** The test aborted it, as the feed does a superseded request. */
  aborted: boolean;
  applied: number;
  otherwise: number;
  abort(): void;
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

type Step = "ask" | "notice" | "answer" | "fail" | "abort";

/**
 * Runs a random interleaving of requests (fresh or not), Change Notices,
 * answers, failures and aborts, with answers landing between any two steps,
 * then answers whatever is left. Records every fresh answer applied in an
 * epoch other than its own in `stale`.
 */
async function interleave(tc: hegel.TestCase) {
  const settler = createSettler(
    () => {},
    () => {},
  );
  const asked: Asked[] = [];
  const stale: string[] = [];
  /** Lets answered requests land, judging first whether each is still wanted. */
  const land = async () => {
    for (const a of asked) {
      if (a.answered && a.wanted === null) a.wanted = !a.signal.aborted && (!a.fresh || a.epoch === settler.epoch);
    }
    await flush();
  };
  const steps = tc.draw(
    gs.arrays(gs.sampledFrom<Step>(["ask", "notice", "answer", "fail", "abort"]), { minSize: 1, maxSize: 40 }),
  );
  for (const step of steps) {
    const open = asked.filter((a) => !a.answered);
    const pick = () => open[tc.draw(gs.integers({ minValue: 0, maxValue: open.length - 1 }))]!;
    switch (step) {
      case "ask": {
        const value = asked.length;
        let resolve!: (v: number) => void;
        let reject!: (e: unknown) => void;
        let signal!: AbortSignal;
        const promise = new Promise<number>((res, rej) => ((resolve = res), (reject = rej)));
        const fresh = tc.draw(gs.booleans());
        const a = {
          fresh,
          epoch: settler.epoch,
          resolve,
          reject,
          answered: false,
          wanted: null,
          failed: false,
          aborted: false,
          applied: 0,
          otherwise: 0,
        } as Omit<Asked, "signal" | "abort"> as Asked;
        const request = settler.settle(
          (s) => ((signal = s), promise),
          (v) => {
            // Recorded, not thrown, as this runs inside a Promise callback.
            if (v !== value || (fresh && a.epoch !== settler.epoch)) {
              stale.push(`#${value} from epoch ${a.epoch} in ${settler.epoch}`);
            }
            a.applied++;
          },
          { fresh, otherwise: () => a.otherwise++ },
        );
        a.signal = signal;
        a.abort = () => request.abort();
        asked.push(a);
        tc.note(`ask #${value}${fresh ? " (fresh)" : ""} in epoch ${a.epoch}`);
        break;
      }
      case "notice":
        settler.bump();
        tc.note(`notice: epoch ${settler.epoch}`);
        break;
      case "answer":
      case "fail": {
        if (!open.length) break;
        const a = pick();
        a.answered = true;
        a.failed = step === "fail";
        if (a.failed) a.wanted = false;
        tc.note(`${step} #${asked.indexOf(a)}`);
        if (step === "answer") a.resolve(asked.indexOf(a));
        else a.reject(new Error("source failed"));
        break;
      }
      case "abort": {
        if (!open.length) break;
        const a = pick();
        tc.note(`abort #${asked.indexOf(a)}`);
        a.aborted = true;
        a.abort();
        break;
      }
    }
    // Answers land at any point between steps, or several steps later.
    if (tc.draw(gs.booleans())) await land();
  }
  for (const a of asked.filter((a) => !a.answered)) {
    a.answered = true;
    a.resolve(asked.indexOf(a));
  }
  await land();
  return { settler, asked, stale };
}

test("settle never applies an answer from an older epoch, under any interleaving of notices and answers", () =>
  hegel.testAsync(async (tc) => {
    const { stale } = await interleave(tc);
    expect(stale).toEqual([]);
  }));

test("every request is settled once: applied if still wanted when it lands, its signal aborted if not", () =>
  hegel.testAsync(async (tc) => {
    const { settler, asked } = await interleave(tc);
    for (const a of asked) {
      expect(a.applied + a.otherwise).toBe(1);
      expect(a.applied).toBe(a.wanted ? 1 : 0);
      if (a.applied) expect(a.signal.aborted).toBe(false);
      // A Change Notice aborts a fresh request in flight, unless the source failed it first.
      else if (a.fresh && !a.failed && a.epoch !== settler.epoch) expect(a.signal.aborted).toBe(true);
      // Only an explicit abort stops one that is not fresh: sorts and Locates outlive notices.
      if (!a.fresh) expect(a.signal.aborted).toBe(a.aborted);
    }
  }));

test("an answer given at once is applied at once, whatever the epoch", () => {
  const settler = createSettler(
    () => {},
    () => {},
  );
  settler.bump();
  let applied: number | null = null;
  settler.settle(() => 7, (v) => (applied = v), { fresh: true });
  expect(applied).toBe(7);
});

test("closing aborts every request in flight and applies nothing that lands after", async () => {
  const settler = createSettler(
    () => {},
    () => {},
  );
  let signal!: AbortSignal;
  let resolve!: (v: number) => void;
  let outcome = "none";
  settler.settle(
    (s) => ((signal = s), new Promise<number>((r) => (resolve = r))),
    () => (outcome = "applied"),
    { otherwise: () => (outcome = "otherwise") },
  );
  settler.close();
  expect(signal.aborted).toBe(true);
  resolve(1);
  await flush();
  expect(outcome).toBe("none");
});
