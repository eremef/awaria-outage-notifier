import Database from "better-sqlite3";
import { readFileSync, unlinkSync, writeFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ASSETS = join(__dirname, "..", "src-tauri", "assets");
const DB_PATH = join(ASSETS, "teryt");

const CSV_FILES = {
  terc: join(ASSETS, "TERC_Urzedowy_2026-03-24.csv"),
  simc: join(ASSETS, "SIMC_Urzedowy_2026-03-24.csv"),
  ulic: join(ASSETS, "ULIC_Urzedowy_2026-03-24.csv"),
};

function parseCsv(filePath) {
  const raw = readFileSync(filePath, "utf-8").replace(/^\uFEFF/, "");
  const lines = raw.replace(/\r\n/g, "\n").trimEnd().split("\n");
  const header = lines[0].split(";");
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    const vals = lines[i].split(";");
    const row = {};
    for (let j = 0; j < header.length; j++) {
      row[header[j]] = vals[j] ?? "";
    }
    rows.push(row);
  }
  return { header, rows };
}

function createSchema(db) {
  db.exec(`
    CREATE TABLE terc (
      woj       INTEGER NOT NULL,
      pow       INTEGER,
      gmi       INTEGER,
      rodz      INTEGER,
      nazwa     TEXT NOT NULL,
      nazwa_dod TEXT,
      stan_na   TEXT NOT NULL
    );

    CREATE TABLE simc (
      woj      INTEGER NOT NULL,
      pow      INTEGER NOT NULL,
      gmi      INTEGER NOT NULL,
      rodz_gmi INTEGER NOT NULL,
      rm       TEXT NOT NULL,
      mz       INTEGER NOT NULL,
      nazwa    TEXT NOT NULL,
      sym      TEXT NOT NULL,
      sympod   TEXT NOT NULL,
      stan_na  TEXT NOT NULL
    );

    CREATE TABLE ulic (
      woj      INTEGER NOT NULL,
      pow      INTEGER NOT NULL,
      gmi      INTEGER NOT NULL,
      rodz_gmi INTEGER NOT NULL,
      sym      TEXT NOT NULL,
      sym_ul   TEXT NOT NULL,
      cecha    TEXT,
      nazwa_1  TEXT NOT NULL,
      nazwa_2  TEXT,
      stan_na  TEXT NOT NULL
    );

    CREATE INDEX idx_terc_codes ON terc (woj, pow, gmi, rodz);

    CREATE INDEX idx_simc_sym ON simc (sym);
    CREATE INDEX idx_simc_codes ON simc (woj, pow, gmi, rodz_gmi);
    CREATE INDEX idx_simc_nazwa ON simc (nazwa);

    CREATE INDEX idx_ulic_sym ON ulic (sym);
    CREATE INDEX idx_ulic_sym_ul ON ulic (sym_ul);
    CREATE INDEX idx_ulic_nazwa ON ulic (nazwa_1);
  `);
}

function loadTerc(db) {
  const { rows } = parseCsv(CSV_FILES.terc);
  const stmt = db.prepare(
    "INSERT INTO terc (woj, pow, gmi, rodz, nazwa, nazwa_dod, stan_na) VALUES (?, ?, ?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.WOJ, 10),
        r.POW ? parseInt(r.POW, 10) : null,
        r.GMI ? parseInt(r.GMI, 10) : null,
        r.RODZ ? parseInt(r.RODZ, 10) : null,
        r.NAZWA,
        r.NAZWA_DOD || null,
        r.STAN_NA
      );
    }
  });
  tx(rows);
  return rows.length;
}

function loadSimc(db) {
  const { rows } = parseCsv(CSV_FILES.simc);
  const stmt = db.prepare(
    "INSERT INTO simc (woj, pow, gmi, rodz_gmi, rm, mz, nazwa, sym, sympod, stan_na) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.WOJ, 10),
        parseInt(r.POW, 10),
        parseInt(r.GMI, 10),
        parseInt(r.RODZ_GMI, 10),
        r.RM,
        parseInt(r.MZ, 10),
        r.NAZWA,
        r.SYM,
        r.SYMPOD,
        r.STAN_NA
      );
    }
  });
  tx(rows);
  return rows.length;
}

function loadUlic(db) {
  const { rows } = parseCsv(CSV_FILES.ulic);
  const stmt = db.prepare(
    "INSERT INTO ulic (woj, pow, gmi, rodz_gmi, sym, sym_ul, cecha, nazwa_1, nazwa_2, stan_na) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.WOJ, 10),
        parseInt(r.POW, 10),
        parseInt(r.GMI, 10),
        parseInt(r.RODZ_GMI, 10),
        r.SYM,
        r.SYM_UL,
        r.CECHA || null,
        r.NAZWA_1,
        r.NAZWA_2 || null,
        r.STAN_NA
      );
    }
  });
  tx(rows);
  return rows.length;
}

// --- Main ---
if (existsSync(DB_PATH)) {
  try {
    unlinkSync(DB_PATH);
    console.log("Removed existing database.");
  } catch {
    writeFileSync(DB_PATH, "");
    console.log("Truncated existing database.");
  }
}

const db = new Database(DB_PATH);
db.pragma("journal_mode = WAL");

console.log("Creating schema...");
createSchema(db);

console.log("Loading TERC...");
const tercCount = loadTerc(db);
console.log(`  → ${tercCount} rows`);

console.log("Loading SIMC...");
const simcCount = loadSimc(db);
console.log(`  → ${simcCount} rows`);

console.log("Loading ULIC...");
const ulicCount = loadUlic(db);
console.log(`  → ${ulicCount} rows`);

db.pragma("journal_mode = DELETE");
db.close();
console.log(`Done. Database saved to ${DB_PATH}`);
