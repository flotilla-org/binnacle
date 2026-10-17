// settle: answers that land late are applied only if no Change Notice came
// in between, and every request's AbortSignal is aborted exactly when its
// answer is no longer wanted.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { expect, test } from "vitest";
import { createSettler } from "../src/settle";

/** A request the test answers when it chooses, as a slow source would. */
interface Asked {
  epoch: number;
  signal: AbortSignal;
  resolve(value: number): void;
  reject(error: unknown): void;
  /** Whether the source has answered yet (it may answer even after an abort). */
  answered: boolean;
  /** No Change Notice and no abort came between asking and the answer landing; `null` until it lands. */
  wanted: boolean | null;
  /** The source rejected it. */
  failed: boolean;
  applied: number;
  otherwise: number;
  abort(): void;
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

type Step = "ask" | "notice" | "answer" | "fail" | "abort";

test("settle never applies an answer from an older epoch, under any interleaving of notices and answers", () =>
  hegel.testAsync(async (tc) => {
    const settler = createSettler(
      () => {},
      () => {},
    );
    const asked: Asked[] = [];
    let id = 0;
    const stale: string[] = [];
    /** Lets answered requests land, judging first whether each is still wanted. */
    const land = async () => {
      for (const a of asked) {
        if (a.answered && a.wanted === null) a.wanted = a.epoch === settler.epoch && !a.signal.aborted;
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
          const value = id++;
          let resolve!: (v: number) => void;
          let reject!: (e: unknown) => void;
          let signal!: AbortSignal;
          const promise = new Promise<number>((res, rej) => ((resolve = res), (reject = rej)));
          const a = {
            epoch: settler.epoch,
            resolve,
            reject,
            answered: false,
            wanted: null,
            failed: false,
            applied: 0,
            otherwise: 0,
          } as Omit<Asked, "signal" | "abort"> as Asked;
          const request = settler.settle(
            (s) => ((signal = s), promise),
            (v) => {
              // The property: the epoch the answer was asked in is the current
              // one. Recorded, not thrown, as this runs inside a Promise callback.
              if (v !== value || a.epoch !== settler.epoch) stale.push(`#${value} from epoch ${a.epoch} in ${settler.epoch}`);
              a.applied++;
            },
            { fresh: true, otherwise: () => a.otherwise++ },
          );
          a.signal = signal;
          a.abort = () => request.abort();
          asked.push(a);
          tc.note(`ask #${value} in epoch ${a.epoch}`);
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
          tc.note(`${step} ask from epoch ${a.epoch}`);
          if (step === "answer") a.resolve(asked.indexOf(a));
          else a.reject(new Error("source failed"));
          break;
        }
        case "abort": {
          if (!open.length) break;
          pick().abort();
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
    expect(stale).toEqual([]);
    for (const a of asked) {
      // Exactly one of applied or otherwise, for every request.
      expect(a.applied + a.otherwise).toBe(1);
      // An answer arriving in its own epoch to a request not aborted is not lost.
      expect(a.applied).toBe(a.wanted ? 1 : 0);
      // A request's signal is aborted once a Change Notice makes its answer unwanted
      // (unless the source failed it first).
      if (!a.failed && a.epoch !== settler.epoch) expect(a.signal.aborted).toBe(a.applied === 0);
      if (a.applied === 1) expect(a.signal.aborted).toBe(false);
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
