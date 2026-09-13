import { describe, expect, it } from "vitest";
import { mergeAsrHotwords, settingsForCourse } from "./courses";

describe("course runtime context", () => {
  it("uses only enabled terms and expands aliases", () => {
    const settings = settingsForCourse(
      {
        id: 7,
        code: "EC1101E",
        name: "Introduction to Economic Analysis",
        description: "Microeconomics",
        createdAt: 1,
      },
      [
        {
          english: "price elasticity",
          chinese: "价格弹性",
          aliases: "PED; elasticity of demand",
          priority: 3,
          enabled: true,
        },
        {
          english: "obsolete term",
          chinese: "旧术语",
          aliases: "",
          priority: 1,
          enabled: false,
        },
      ],
    );

    expect(settings.keywords).toEqual([
      "price elasticity",
      "PED",
      "elasticity of demand",
    ]);
    expect(settings.hotwords).toEqual([
      { text: "price elasticity", weight: 5 },
      { text: "PED", weight: 5 },
      { text: "elasticity of demand", weight: 5 },
    ]);
    expect(settings.glossary[0]).toContain("price elasticity → 价格弹性");
    expect(settings.glossary.join(" ")).not.toContain("obsolete term");
  });

  it("keeps preferred weights when slide keywords overlap", () => {
    expect(mergeAsrHotwords(
      [{ text: "Singtel", weight: 5 }],
      ["singtel", "StarHub"],
    )).toEqual([
      { text: "Singtel", weight: 5 },
      { text: "StarHub", weight: 2 },
    ]);
  });

  it("splits aliases written with Chinese punctuation", () => {
    const settings = settingsForCourse(
      {
        id: 8,
        code: "CEG",
        name: "Cellular Networks",
        description: "",
        createdAt: 1,
      },
      [{
        english: "Singtel",
        chinese: "新电信",
        aliases: "Singapore Telecom，Singtel Ltd；SingTel",
        priority: 3,
        enabled: true,
      }],
    );

    expect(settings.keywords).toEqual([
      "Singtel",
      "Singapore Telecom",
      "Singtel Ltd",
    ]);
  });
});
