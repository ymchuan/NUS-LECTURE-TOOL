import { describe, expect, it } from "vitest";
import {
  CELLULAR_NETWORK_GLOSSARY,
  MACHINE_LEARNING_GLOSSARY,
  mergeGlossaryTerms,
  recommendedGlossaryPreset,
} from "./glossaryPresets";

describe("course glossary presets", () => {
  it("recognizes CEG5104 and includes local operator names", () => {
    const preset = recommendedGlossaryPreset({
      code: "CEG5104",
      name: "Cellular Networks",
      description: "Capacity planning and radio access networks",
    });
    expect(preset?.terms.map((item) => item.english)).toEqual(
      expect.arrayContaining(["Singtel", "StarHub", "Erlang B", "M/M/c/c queue"]),
    );
    expect(CELLULAR_NETWORK_GLOSSARY.length).toBeGreaterThan(100);
  });

  it("recognizes CEG5301 and includes neural-network terminology", () => {
    const preset = recommendedGlossaryPreset({
      code: "CEG5301",
      name: "Machine Learning with Applications",
      description: "Neural networks and statistical learning",
    });
    expect(preset?.terms.map((item) => item.english)).toEqual(
      expect.arrayContaining([
        "perceptron",
        "synaptic weights",
        "induced local field",
        "backpropagation",
        "operant conditioning",
      ]),
    );
    expect(MACHINE_LEARNING_GLOSSARY.length).toBeGreaterThan(70);
  });

  it("adds missing suggestions without overwriting an existing term", () => {
    const existing = [{
      english: "Singtel",
      chinese: "用户自定义译法",
      aliases: "",
      priority: 1,
      enabled: true,
    }];
    const merged = mergeGlossaryTerms(existing, CELLULAR_NETWORK_GLOSSARY);
    expect(merged.terms[0]).toEqual(existing[0]);
    expect(merged.terms.filter((item) => item.english === "Singtel")).toHaveLength(1);
    expect(merged.added).toBe(CELLULAR_NETWORK_GLOSSARY.length - 1);
  });
});
