const MAX_UPLOAD_BYTES = 16 * 1024 * 1024;
const WASM_BUNDLE_BASENAME = "fallout-se-web";
const GAME_TIME_TICKS_PER_YEAR = 315_360_000;
const STAT_AGE_INDEX = 33;

const SPECIAL_NAMES = [
  "Strength", "Perception", "Endurance", "Charisma",
  "Intelligence", "Agility", "Luck",
];

const TRAIT_NAMES = [
  "Fast Metabolism", "Bruiser", "Small Frame", "One Hander",
  "Finesse", "Kamikaze", "Heavy Handed", "Fast Shot",
  "Bloody Mess", "Jinxed", "Good Natured", "Chem Reliant",
  "Chem Resistant", "Night Person", "Skilled", "Gifted",
];

const SKILL_NAMES = [
  "Small Guns", "Big Guns", "Energy Weapons", "Unarmed",
  "Melee Weapons", "Throwing", "First Aid", "Doctor",
  "Sneak", "Lockpick", "Steal", "Traps",
  "Science", "Repair", "Speech", "Barter",
  "Gambling", "Outdoorsman",
];

const F1_PERK_NAMES = [
  "Awareness", "Bonus HtH Attacks", "Bonus HtH Damage", "Bonus Move",
  "Bonus Ranged Damage", "Bonus Rate of Fire", "Earlier Sequence", "Faster Healing",
  "More Criticals", "Night Vision", "Presence", "Rad Resistance",
  "Toughness", "Strong Back", "Sharpshooter", "Silent Running",
  "Survivalist", "Master Trader", "Educated", "Healer",
  "Fortune Finder", "Better Criticals", "Empathy", "Slayer",
  "Sniper", "Silent Death", "Action Boy", "Mental Block",
  "Lifegiver", "Dodger", "Snakeater", "Mr. Fixit",
  "Medic", "Master Thief", "Speaker", "Heave Ho!",
  "Friendly Foe", "Pickpocket", "Ghost", "Cult of Personality",
  "Scrounger", "Explorer", "Flower Child", "Pathfinder",
  "Animal Friend", "Scout", "Mysterious Stranger", "Ranger",
  "Quick Pockets", "Smooth Talker", "Swift Learner", "Tag!",
  "Mutate!", "Nuka-Cola Addiction", "Buffout Addiction", "Mentats Addiction",
  "Psycho Addiction", "Radaway Addiction", "Weapon Long Range", "Weapon Accurate",
  "Weapon Penetrate", "Weapon Knockback", "Powered Armor", "Combat Armor",
];

function titleCase(s) {
  return s.toLowerCase().replace(/(?:^|\s)\S/g, (c) => c.toUpperCase());
}

const F2_PERK_NAMES_RAW = [
  "AWARENESS", "BONUS HTH ATTACKS", "BONUS HTH DAMAGE", "BONUS MOVE",
  "BONUS RANGED DAMAGE", "BONUS RATE OF FIRE", "EARLIER SEQUENCE", "FASTER HEALING",
  "MORE CRITICALS", "NIGHT VISION", "PRESENCE", "RAD RESISTANCE",
  "TOUGHNESS", "STRONG BACK", "SHARPSHOOTER", "SILENT RUNNING",
  "SURVIVALIST", "MASTER TRADER", "EDUCATED", "HEALER",
  "FORTUNE FINDER", "BETTER CRITICALS", "EMPATHY", "SLAYER",
  "SNIPER", "SILENT DEATH", "ACTION BOY", "MENTAL BLOCK",
  "LIFEGIVER", "DODGER", "SNAKEATER", "MR FIXIT",
  "MEDIC", "MASTER THIEF", "SPEAKER", "HEAVE HO",
  "FRIENDLY FOE", "PICKPOCKET", "GHOST", "CULT OF PERSONALITY",
  "SCROUNGER", "EXPLORER", "FLOWER CHILD", "PATHFINDER",
  "ANIMAL FRIEND", "SCOUT", "MYSTERIOUS STRANGER", "RANGER",
  "QUICK POCKETS", "SMOOTH TALKER", "SWIFT LEARNER", "TAG",
  "MUTATE", "NUKA COLA ADDICTION", "BUFFOUT ADDICTION", "MENTATS ADDICTION",
  "PSYCHO ADDICTION", "RADAWAY ADDICTION", "WEAPON LONG RANGE", "WEAPON ACCURATE",
  "WEAPON PENETRATE", "WEAPON KNOCKBACK", "POWERED ARMOR", "COMBAT ARMOR",
  "WEAPON SCOPE RANGE", "WEAPON FAST RELOAD", "WEAPON NIGHT SIGHT", "WEAPON FLAMEBOY",
  "ARMOR ADVANCED I", "ARMOR ADVANCED II", "JET ADDICTION", "TRAGIC ADDICTION",
  "ARMOR CHARISMA", "GECKO SKINNING", "DERMAL IMPACT ARMOR",
  "DERMAL IMPACT ASSAULT ENHANCEMENT", "PHOENIX ARMOR IMPLANTS",
  "PHOENIX ASSAULT ENHANCEMENT", "VAULT CITY INOCULATIONS",
  "ADRENALINE RUSH", "CAUTIOUS NATURE", "COMPREHENSION", "DEMOLITION EXPERT",
  "GAMBLER", "GAIN STRENGTH", "GAIN PERCEPTION", "GAIN ENDURANCE",
  "GAIN CHARISMA", "GAIN INTELLIGENCE", "GAIN AGILITY", "GAIN LUCK",
  "HARMLESS", "HERE AND NOW", "HTH EVADE", "KAMA SUTRA MASTER",
  "KARMA BEACON", "LIGHT STEP", "LIVING ANATOMY", "MAGNETIC PERSONALITY",
  "NEGOTIATOR", "PACK RAT", "PYROMANIAC", "QUICK RECOVERY",
  "SALESMAN", "STONEWALL", "THIEF", "WEAPON HANDLING",
  "VAULT CITY TRAINING", "ALCOHOL RAISED HIT POINTS", "ALCOHOL RAISED HIT POINTS II",
  "ALCOHOL LOWERED HIT POINTS", "ALCOHOL LOWERED HIT POINTS II",
  "AUTODOC RAISED HIT POINTS", "AUTODOC RAISED HIT POINTS II",
  "AUTODOC LOWERED HIT POINTS", "AUTODOC LOWERED HIT POINTS II",
  "EXPERT EXCREMENT EXPEDITOR", "WEAPON ENHANCED KNOCKOUT", "JINXED",
];
const F2_PERK_NAMES = F2_PERK_NAMES_RAW.map(titleCase);

let wasmBindings = null;

const elements = {
  dropZone: document.getElementById("drop-zone"),
  fileInput: document.getElementById("file-input"),
  chooseFile: document.getElementById("choose-file"),
  copyJson: document.getElementById("copy-json"),
  formatJson: document.getElementById("format-json"),
  applyJson: document.getElementById("apply-json"),
  downloadSave: document.getElementById("download-save"),
  jsonEditor: document.getElementById("json-editor"),
  status: document.getElementById("status"),
  formEditor: document.getElementById("form-editor"),
  jsonDetails: document.getElementById("json-details"),
  // Save info
  fiGame: document.getElementById("fi-game"),
  fiName: document.getElementById("fi-name"),
  fiDescription: document.getElementById("fi-description"),
  fiMap: document.getElementById("fi-map"),
  fiMapId: document.getElementById("fi-map-id"),
  fiElevation: document.getElementById("fi-elevation"),
  fiGameDate: document.getElementById("fi-game-date"),
  fiSaveDate: document.getElementById("fi-save-date"),
  fiGameTime: document.getElementById("fi-game-time"),
  fiGlobalVarCount: document.getElementById("fi-global-var-count"),
  fiNextLevelXp: document.getElementById("fi-next-level-xp"),
  // Editable core stats
  fiGender: document.getElementById("fi-gender"),
  fiLevel: document.getElementById("fi-level"),
  fiBaseAge: document.getElementById("fi-base-age"),
  fiXp: document.getElementById("fi-xp"),
  fiSkillPoints: document.getElementById("fi-skill-points"),
  fiHp: document.getElementById("fi-hp"),
  fiKarma: document.getElementById("fi-karma"),
  fiReputation: document.getElementById("fi-reputation"),
  // Containers
  specialRows: document.getElementById("special-rows"),
  fiTrait0: document.getElementById("fi-trait-0"),
  fiTrait1: document.getElementById("fi-trait-1"),
  perkRows: document.getElementById("perk-rows"),
  addPerk: document.getElementById("add-perk"),
  skillRows: document.getElementById("skill-rows"),
  statRows: document.getElementById("stat-rows"),
  killCountRows: document.getElementById("kill-count-rows"),
  taggedSkillRows: document.getElementById("tagged-skill-rows"),
  inventoryRows: document.getElementById("inventory-rows"),
  addInventory: document.getElementById("add-inventory"),
};

const state = {
  originalBytes: null,
  updatedBytes: null,
  filenameBase: "save",
  outputFilename: "SAVE_edited.DAT",
  ready: false,
  initError: null,
  currentJson: null,
  syncDirection: null,
};

let jsonSyncTimer = null;

function setStatus(message, kind = "info") {
  elements.status.textContent = message;
  elements.status.dataset.kind = kind;
}

function normalizeFilename(value) {
  const cleaned = value.replace(/\.[^/.]+$/, "").replace(/[^a-zA-Z0-9_-]+/g, "_");
  return cleaned.length > 0 ? cleaned : "save";
}

function formatDate(d) {
  if (!d) return "—";
  return `${d.year}-${String(d.month).padStart(2, "0")}-${String(d.day).padStart(2, "0")}`;
}

function elapsedGameYears(gameTime) {
  const parsed = Number(gameTime);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 0;
  }
  return Math.floor(parsed / GAME_TIME_TICKS_PER_YEAR);
}

function findAgeStat(stats) {
  if (!Array.isArray(stats)) {
    return null;
  }
  return stats.find((s) => s && s.index === STAT_AGE_INDEX) || null;
}

function baseAgeFromJson(json) {
  const ageStat = findAgeStat(json.stats);
  if (!ageStat) {
    return "";
  }

  const effectiveTotal = Number(ageStat.total) || 0;
  const bonus = Number(ageStat.bonus) || 0;
  return effectiveTotal - bonus - elapsedGameYears(json.game_time);
}

function getPerkNamesForGame() {
  if (state.currentJson && state.currentJson.game === "Fallout2") {
    return F2_PERK_NAMES;
  }
  return F1_PERK_NAMES;
}

function populateSelect(selectEl, names, selectedName) {
  selectEl.innerHTML = "";
  const noneOpt = document.createElement("option");
  noneOpt.value = "";
  noneOpt.textContent = "None";
  selectEl.appendChild(noneOpt);
  for (const name of names) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    selectEl.appendChild(opt);
  }
  selectEl.value = selectedName || "";
}

function populateTraitSelects(traits) {
  populateSelect(elements.fiTrait0, TRAIT_NAMES, traits[0]?.name || "");
  populateSelect(elements.fiTrait1, TRAIT_NAMES, traits[1]?.name || "");
}

function buildSpecialRows(special) {
  elements.specialRows.innerHTML = "";
  for (let i = 0; i < SPECIAL_NAMES.length; i++) {
    const entry = special[i] || { base: 5, bonus: 0, total: 5 };
    const row = document.createElement("div");
    row.className = "field-row";
    row.innerHTML =
      `<label>${SPECIAL_NAMES[i]}</label>` +
      `<input type="number" class="special-base" data-index="${i}" value="${entry.base}" min="1" max="10">` +
      `<span class="read-only">bonus: ${entry.bonus}</span>` +
      `<span class="read-only">total: ${entry.total}</span>`;
    elements.specialRows.appendChild(row);
  }
}

function addPerkRow(perkNames, index, rank) {
  const row = document.createElement("div");
  row.className = "dynamic-row";
  const sel = document.createElement("select");
  sel.className = "perk-name";
  for (let i = 0; i < perkNames.length; i++) {
    const opt = document.createElement("option");
    opt.value = i;
    opt.textContent = perkNames[i];
    sel.appendChild(opt);
  }
  if (index != null) sel.value = String(index);
  const rankInput = document.createElement("input");
  rankInput.type = "number";
  rankInput.className = "perk-rank";
  rankInput.min = "1";
  rankInput.value = rank != null ? rank : 1;
  const removeBtn = document.createElement("button");
  removeBtn.type = "button";
  removeBtn.className = "remove-btn";
  removeBtn.textContent = "X";
  removeBtn.addEventListener("click", () => {
    row.remove();
    syncFormToJson();
  });
  const label = document.createElement("span");
  label.className = "read-only";
  label.textContent = "rank:";
  sel.addEventListener("change", () => syncFormToJson());
  rankInput.addEventListener("input", () => syncFormToJson());
  row.appendChild(sel);
  row.appendChild(label);
  row.appendChild(rankInput);
  row.appendChild(removeBtn);
  elements.perkRows.appendChild(row);
}

function addInventoryRow(quantity, pid) {
  const row = document.createElement("div");
  row.className = "dynamic-row";
  const qtyLabel = document.createElement("span");
  qtyLabel.className = "read-only";
  qtyLabel.textContent = "qty:";
  const qtyInput = document.createElement("input");
  qtyInput.type = "number";
  qtyInput.className = "inv-quantity";
  qtyInput.min = "1";
  qtyInput.value = quantity != null ? quantity : 1;
  const pidLabel = document.createElement("span");
  pidLabel.className = "read-only";
  pidLabel.textContent = "PID:";
  const pidInput = document.createElement("input");
  pidInput.type = "number";
  pidInput.className = "inv-pid";
  pidInput.value = pid != null ? pid : 0;
  const removeBtn = document.createElement("button");
  removeBtn.type = "button";
  removeBtn.className = "remove-btn";
  removeBtn.textContent = "X";
  removeBtn.addEventListener("click", () => {
    row.remove();
    syncFormToJson();
  });
  qtyInput.addEventListener("input", () => syncFormToJson());
  pidInput.addEventListener("input", () => syncFormToJson());
  row.appendChild(qtyLabel);
  row.appendChild(qtyInput);
  row.appendChild(pidLabel);
  row.appendChild(pidInput);
  row.appendChild(removeBtn);
  elements.inventoryRows.appendChild(row);
}

function buildSkillRows(skills) {
  elements.skillRows.innerHTML = "";
  for (const s of skills) {
    const row = document.createElement("div");
    row.className = "field-row";
    row.innerHTML =
      `<label>${s.name}</label>` +
      `<span class="read-only">raw:</span>` +
      `<input type="number" class="skill-raw" data-index="${s.index}" value="${s.raw}">` +
      `<span class="read-only">tag: ${s.tag_bonus}</span>` +
      `<span class="read-only">bonus: ${s.bonus}</span>` +
      `<span class="read-only">total: ${s.total}</span>`;
    elements.skillRows.appendChild(row);
  }
}

function buildStatRows(stats) {
  elements.statRows.innerHTML = "";
  for (const s of stats) {
    const row = document.createElement("div");
    row.className = "field-row";
    row.innerHTML =
      `<label>${s.name}</label>` +
      `<span class="read-only">base: ${s.base}</span>` +
      `<span class="read-only">bonus: ${s.bonus}</span>` +
      `<span class="read-only">total: ${s.total}</span>`;
    elements.statRows.appendChild(row);
  }
}

function buildKillCountRows(kills) {
  elements.killCountRows.innerHTML = "";
  if (!kills || kills.length === 0) {
    elements.killCountRows.innerHTML = '<span class="read-only">No kills recorded.</span>';
    return;
  }
  for (const k of kills) {
    const row = document.createElement("div");
    row.className = "field-row";
    row.innerHTML = `<label>${k.name}</label><span class="read-only">${k.count}</span>`;
    elements.killCountRows.appendChild(row);
  }
}

function buildTaggedSkillRows(taggedSkills, skills) {
  elements.taggedSkillRows.innerHTML = "";
  if (!taggedSkills || taggedSkills.length === 0) {
    elements.taggedSkillRows.innerHTML = '<span class="read-only">None</span>';
    return;
  }
  for (const idx of taggedSkills) {
    const name = skills.find((s) => s.index === idx)?.name || SKILL_NAMES[idx] || `Skill #${idx}`;
    const row = document.createElement("div");
    row.className = "field-row";
    row.innerHTML = `<span class="read-only">${name}</span>`;
    elements.taggedSkillRows.appendChild(row);
  }
}

function populateFormFromJson(json) {
  // Save info
  elements.fiGame.textContent = json.game || "—";
  elements.fiName.value = json.name || "";
  elements.fiDescription.value = json.description || "";
  elements.fiMap.textContent = json.map || "—";
  elements.fiMapId.textContent = json.map_id ?? "—";
  elements.fiElevation.textContent = json.elevation ?? "—";
  elements.fiGameDate.textContent = formatDate(json.game_date);
  elements.fiSaveDate.textContent = formatDate(json.save_date);
  elements.fiGameTime.textContent = json.game_time ?? "—";
  elements.fiGlobalVarCount.textContent = json.global_var_count ?? "—";
  elements.fiNextLevelXp.textContent = json.next_level_xp ?? "—";

  // Core stats (editable)
  elements.fiGender.value = json.gender || "Male";
  elements.fiLevel.value = json.level ?? "";
  elements.fiBaseAge.value = baseAgeFromJson(json);
  elements.fiXp.value = json.xp ?? "";
  elements.fiSkillPoints.value = json.skill_points ?? "";
  elements.fiHp.value = json.hp ?? "";
  elements.fiKarma.value = json.karma ?? "";
  elements.fiReputation.value = json.reputation ?? "";

  // S.P.E.C.I.A.L.
  buildSpecialRows(json.special || []);

  // Traits
  populateTraitSelects(json.traits || []);

  // Perks
  elements.perkRows.innerHTML = "";
  const perkNames = getPerkNamesForGame();
  for (const p of json.perks || []) {
    addPerkRow(perkNames, p.index, p.rank);
  }

  // Skills
  buildSkillRows(json.skills || []);

  // Derived stats (read-only)
  buildStatRows(json.stats || []);

  // Kill counts (read-only)
  buildKillCountRows(json.kill_counts || []);

  // Tagged skills (read-only)
  buildTaggedSkillRows(json.tagged_skills || [], json.skills || []);

  // Inventory
  elements.inventoryRows.innerHTML = "";
  for (const item of json.inventory || []) {
    addInventoryRow(item.quantity, item.pid);
  }
}

function readTraitFromSelect(selectEl) {
  const name = selectEl.value;
  if (!name) return null;
  const idx = TRAIT_NAMES.indexOf(name);
  if (idx < 0) return null;
  return { index: idx, name };
}

function syncFormToJson() {
  if (state.syncDirection === "json") return;
  if (!state.currentJson) return;
  state.syncDirection = "form";

  const json = state.currentJson;

  // Save info
  json.name = elements.fiName.value;
  json.description = elements.fiDescription.value;

  // Core stats
  json.gender = elements.fiGender.value;
  json.level = parseInt(elements.fiLevel.value, 10) || 0;
  json.xp = parseInt(elements.fiXp.value, 10) || 0;
  json.skill_points = parseInt(elements.fiSkillPoints.value, 10) || 0;
  json.hp = parseInt(elements.fiHp.value, 10) || 0;
  json.karma = parseInt(elements.fiKarma.value, 10) || 0;
  json.reputation = parseInt(elements.fiReputation.value, 10) || 0;
  const ageStat = findAgeStat(json.stats);
  if (ageStat) {
    const baseAge = parseInt(elements.fiBaseAge.value, 10) || 0;
    const ageBonus = Number(ageStat.bonus) || 0;
    ageStat.base = baseAge;
    ageStat.total = baseAge + ageBonus + elapsedGameYears(json.game_time);
  }

  // S.P.E.C.I.A.L. base values
  const baseInputs = elements.specialRows.querySelectorAll(".special-base");
  for (const inp of baseInputs) {
    const i = parseInt(inp.dataset.index, 10);
    if (json.special[i]) {
      json.special[i].base = parseInt(inp.value, 10) || 1;
    }
  }

  // Traits
  const traits = [];
  const t0 = readTraitFromSelect(elements.fiTrait0);
  const t1 = readTraitFromSelect(elements.fiTrait1);
  if (t0) traits.push(t0);
  if (t1) traits.push(t1);
  json.traits = traits;

  // Perks
  const perkNames = getPerkNamesForGame();
  const perkRowEls = elements.perkRows.querySelectorAll(".dynamic-row");
  json.perks = [];
  for (const row of perkRowEls) {
    const sel = row.querySelector(".perk-name");
    const rankInp = row.querySelector(".perk-rank");
    const index = parseInt(sel.value, 10);
    const rank = parseInt(rankInp.value, 10) || 1;
    const name = perkNames[index] || `Perk #${index}`;
    json.perks.push({ index, name, rank });
  }

  // Skills raw/base values
  const skillRawInputs = elements.skillRows.querySelectorAll(".skill-raw");
  for (const inp of skillRawInputs) {
    const skillIndex = parseInt(inp.dataset.index, 10);
    const skill = (json.skills || []).find((s) => s.index === skillIndex);
    if (skill) {
      skill.raw = parseInt(inp.value, 10) || 0;
    }
  }

  // Inventory
  const invRowEls = elements.inventoryRows.querySelectorAll(".dynamic-row");
  json.inventory = [];
  for (const row of invRowEls) {
    const qty = parseInt(row.querySelector(".inv-quantity").value, 10) || 1;
    const pid = parseInt(row.querySelector(".inv-pid").value, 10) || 0;
    json.inventory.push({ quantity: qty, pid });
  }

  elements.jsonEditor.value = JSON.stringify(json, null, 2);
  state.syncDirection = null;
}

function syncJsonToForm() {
  if (state.syncDirection === "form") return;
  state.syncDirection = "json";
  try {
    const parsed = JSON.parse(elements.jsonEditor.value);
    populateFormFromJson(parsed);
    state.currentJson = parsed;
  } catch (_e) {
    // Invalid JSON while user is typing — ignore silently
  }
  state.syncDirection = null;
}

function resetEditor() {
  state.originalBytes = null;
  state.updatedBytes = null;
  state.filenameBase = "save";
  state.outputFilename = "SAVE_edited.DAT";
  state.currentJson = null;
  state.syncDirection = null;
  elements.jsonEditor.value = "";
  elements.copyJson.disabled = true;
  elements.formatJson.disabled = true;
  elements.applyJson.disabled = true;
  elements.downloadSave.disabled = true;
  elements.formEditor.classList.remove("visible");
}

function extractErrorMessage(error) {
  if (error && typeof error === "object") {
    if (typeof error.message === "string") {
      return error.message;
    }
    if (typeof error.code === "string") {
      return `Web editor error: ${error.code}`;
    }
  }
  return String(error ?? "Unknown error");
}

function baseHref() {
  const base = document.querySelector("base");
  if (base && typeof base.href === "string" && base.href.length > 0) {
    return base.href;
  }
  return document.baseURI;
}

function coerceUint8Array(value) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) {
    return new Uint8Array(value);
  }
  return null;
}

async function loadSaveFile(file) {
  if (state.initError) {
    setStatus(`WASM failed to load: ${state.initError}`, "error");
    return;
  }

  if (!state.ready) {
    setStatus("WASM engine is still loading...", "info");
    return;
  }

  if (!file) {
    setStatus("No file selected.", "error");
    return;
  }

  if (file.size > MAX_UPLOAD_BYTES) {
    resetEditor();
    setStatus("File is too large. Maximum supported size is 16 MiB.", "error");
    return;
  }

  const lower = file.name.toLowerCase();
  if (!lower.endsWith("save.dat") && !lower.endsWith(".dat")) {
    setStatus("Selected file does not look like SAVE.DAT; attempting parse anyway.", "info");
  } else {
    setStatus(`Reading ${file.name}...`, "info");
  }

  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const options = {
      metadata: null,
    };
    const renderedJson = wasmBindings.export_save_json(bytes, options);

    state.originalBytes = bytes;
    state.updatedBytes = null;
    state.filenameBase = normalizeFilename(file.name);
    state.outputFilename = `${state.filenameBase}_edited.SAVE.DAT`;

    elements.jsonEditor.value = renderedJson;
    elements.copyJson.disabled = false;
    elements.formatJson.disabled = false;
    elements.applyJson.disabled = false;
    elements.downloadSave.disabled = true;

    state.currentJson = JSON.parse(renderedJson);
    populateFormFromJson(state.currentJson);
    elements.formEditor.classList.add("visible");

    setStatus(`Loaded ${file.name}. Edit fields or JSON and apply when ready.`, "ok");
  } catch (error) {
    resetEditor();
    setStatus(extractErrorMessage(error), "error");
  }
}

async function copyJson() {
  const text = elements.jsonEditor.value;
  if (!text) {
    return;
  }

  try {
    await navigator.clipboard.writeText(text);
    setStatus("JSON copied to clipboard.", "ok");
  } catch (_error) {
    setStatus("Clipboard access denied by the browser.", "error");
  }
}

function formatJson() {
  const text = elements.jsonEditor.value.trim();
  if (!text) {
    setStatus("No JSON to format.", "error");
    return;
  }

  try {
    const parsed = JSON.parse(text);
    elements.jsonEditor.value = JSON.stringify(parsed, null, 2);
    setStatus("JSON formatted.", "ok");
  } catch (error) {
    setStatus(`Invalid JSON: ${extractErrorMessage(error)}`, "error");
  }
}

function parseApplyPayload(payload) {
  if (!payload || typeof payload !== "object") {
    throw new Error("Invalid apply response from wasm.");
  }

  const bytes = coerceUint8Array(payload.updated_bytes);
  if (!bytes || bytes.length === 0) {
    throw new Error("Apply response did not include updated save bytes.");
  }

  const filenameHint =
    typeof payload.filename_hint === "string" && payload.filename_hint.length > 0
      ? payload.filename_hint
      : `${state.filenameBase}_edited.SAVE.DAT`;

  return {
    bytes,
    filenameHint,
  };
}

function applyJsonAndBuildSave() {
  if (!state.originalBytes || state.originalBytes.length === 0) {
    setStatus("Load a SAVE.DAT before applying JSON edits.", "error");
    return;
  }

  const editedJson = elements.jsonEditor.value.trim();
  if (!editedJson) {
    setStatus("Editor JSON is empty.", "error");
    return;
  }

  try {
    const options = {
      metadata: null,
    };
    const payload = wasmBindings.apply_json_to_save(state.originalBytes, editedJson, options);
    const { bytes, filenameHint } = parseApplyPayload(payload);

    state.updatedBytes = bytes;
    state.outputFilename = filenameHint;
    elements.downloadSave.disabled = false;
    setStatus(`Applied JSON edits. Ready to download ${state.outputFilename}.`, "ok");
  } catch (error) {
    state.updatedBytes = null;
    elements.downloadSave.disabled = true;
    setStatus(extractErrorMessage(error), "error");
  }
}

function downloadEditedSave() {
  if (!state.updatedBytes || state.updatedBytes.length === 0) {
    return;
  }

  const blob = new Blob([state.updatedBytes], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = state.outputFilename;
  anchor.click();
  URL.revokeObjectURL(url);
  setStatus(`Saved ${state.outputFilename}.`, "ok");
}

function wireDragAndDrop() {
  const activate = () => elements.dropZone.classList.add("active");
  const deactivate = () => elements.dropZone.classList.remove("active");

  ["dragenter", "dragover"].forEach((eventName) => {
    elements.dropZone.addEventListener(eventName, (event) => {
      event.preventDefault();
      activate();
    });
  });

  ["dragleave", "dragend"].forEach((eventName) => {
    elements.dropZone.addEventListener(eventName, (event) => {
      event.preventDefault();
      deactivate();
    });
  });

  elements.dropZone.addEventListener("drop", (event) => {
    event.preventDefault();
    deactivate();
    const file = event.dataTransfer?.files?.[0];
    void loadSaveFile(file);
  });

  elements.dropZone.addEventListener("click", () => {
    elements.fileInput.click();
  });

  elements.dropZone.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      elements.fileInput.click();
    }
  });
}

function wireFormEvents() {
  // Editable save info/core stat inputs
  const coreInputs = [
    elements.fiName, elements.fiDescription,
    elements.fiGender, elements.fiLevel, elements.fiBaseAge, elements.fiXp,
    elements.fiSkillPoints, elements.fiHp, elements.fiKarma, elements.fiReputation,
  ];
  for (const el of coreInputs) {
    const eventName = el.tagName === "SELECT" ? "change" : "input";
    el.addEventListener(eventName, () => syncFormToJson());
  }

  // Trait selects
  elements.fiTrait0.addEventListener("change", () => syncFormToJson());
  elements.fiTrait1.addEventListener("change", () => syncFormToJson());

  // S.P.E.C.I.A.L. base inputs (delegated)
  elements.specialRows.addEventListener("input", (e) => {
    if (e.target.classList.contains("special-base")) {
      syncFormToJson();
    }
  });

  // Skill raw inputs (delegated)
  elements.skillRows.addEventListener("input", (e) => {
    if (e.target.classList.contains("skill-raw")) {
      syncFormToJson();
    }
  });

  // Add perk button
  elements.addPerk.addEventListener("click", () => {
    const perkNames = getPerkNamesForGame();
    addPerkRow(perkNames, 0, 1);
    syncFormToJson();
  });

  // Add inventory button
  elements.addInventory.addEventListener("click", () => {
    addInventoryRow(1, 0);
    syncFormToJson();
  });

  // JSON textarea → form (debounced)
  elements.jsonEditor.addEventListener("input", () => {
    clearTimeout(jsonSyncTimer);
    jsonSyncTimer = setTimeout(syncJsonToForm, 300);
  });
}

function wireControls() {
  elements.chooseFile.addEventListener("click", () => elements.fileInput.click());
  elements.fileInput.addEventListener("change", () => {
    const file = elements.fileInput.files?.[0];
    void loadSaveFile(file);
  });
  elements.copyJson.addEventListener("click", () => {
    void copyJson();
  });
  elements.formatJson.addEventListener("click", formatJson);
  elements.applyJson.addEventListener("click", applyJsonAndBuildSave);
  elements.downloadSave.addEventListener("click", downloadEditedSave);
}

async function initWasm() {
  async function loadWasmModule(forceFresh) {
    const base = baseHref();
    const cacheSuffix = forceFresh ? `?v=${Date.now()}` : "";
    const jsUrl = new URL(`${WASM_BUNDLE_BASENAME}.js${cacheSuffix}`, base).toString();
    const wasmUrl = new URL(`${WASM_BUNDLE_BASENAME}_bg.wasm${cacheSuffix}`, base).toString();

    const mod = await import(jsUrl);
    if (typeof mod.default !== "function") {
      throw new Error(`WASM bundle missing default init(): ${jsUrl}`);
    }
    await mod.default({ module_or_path: wasmUrl });
    return mod;
  }

  function hasEditorBindings(bindings) {
    return (
      bindings &&
      typeof bindings.export_save_json === "function" &&
      typeof bindings.apply_json_to_save === "function"
    );
  }

  let mod = await loadWasmModule(false);
  if (!hasEditorBindings(mod)) {
    // Retry with cache-busting query params so stale wasm/js bundles are not reused.
    mod = await loadWasmModule(true);
  }

  if (!hasEditorBindings(mod)) {
    throw new Error(
      "Loaded WASM module does not expose editor APIs (export_save_json/apply_json_to_save). Hard refresh the page to clear stale assets.",
    );
  }

  wasmBindings = mod;
  state.ready = true;
}

async function main() {
  resetEditor();
  wireDragAndDrop();
  wireControls();
  wireFormEvents();
  setStatus("Loading WASM editor...", "info");

  try {
    await initWasm();
    setStatus("Drop a SAVE.DAT file to begin editing.", "info");
  } catch (error) {
    const message = extractErrorMessage(error);
    state.initError = message;
    setStatus(`Failed to initialize WASM editor: ${message}`, "error");
  }
}

void main();
