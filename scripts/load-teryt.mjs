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
      nazwa_dod TEXT
    );

    CREATE TABLE simc (
      woj      INTEGER NOT NULL,
      pow      INTEGER NOT NULL,
      gmi      INTEGER NOT NULL,
      rodz_gmi INTEGER NOT NULL,
      rm       INTEGER NOT NULL,
      nazwa    TEXT NOT NULL,
      sym      INTEGER NOT NULL,
      sympod   INTEGER NOT NULL
    );

    CREATE TABLE ulic (
      sym      INTEGER NOT NULL,
      sym_ul   INTEGER NOT NULL,
      cecha    TEXT,
      nazwa_1  TEXT NOT NULL,
      nazwa_2  TEXT
    );

    CREATE TABLE rodz_miej (
      rm      INTEGER NOT NULL,
      nazwa   TEXT NOT NULL
    );    

    CREATE TABLE rodz_gmi (
      rodz_gmi INTEGER NOT NULL,
      nazwa   TEXT NOT NULL
    );    

    CREATE INDEX idx_terc_codes ON terc (woj, pow, gmi, rodz);

    CREATE INDEX idx_simc_sym ON simc (sym);
    CREATE INDEX idx_simc_codes ON simc (woj, pow, gmi, rodz_gmi);
    CREATE INDEX idx_simc_nazwa ON simc (nazwa);

    CREATE INDEX idx_ulic_sym ON ulic (sym);
    CREATE INDEX idx_ulic_nazwa ON ulic (nazwa_1);
  `);
}

function loadDictionaries(db) {
  db.exec(`INSERT INTO rodz_miej (rm, nazwa) VALUES (0, 'część miejscowości'),(1, 'wieś'),(2, 'kolonia'),(3, 'przysiółek'),(4, 'osada'),(5, 'osada leśna'),(6, 'osiedle'),(7, 'schronisko turystyczne'),(95, 'dzielnica m. st. Warszawy'),(96, 'miasto'),(98, 'delegatura'),(99, 'część miasta')`);
  db.exec(`INSERT INTO rodz_gmi (rodz_gmi, nazwa) VALUES (1, 'gmina miejska'),(2, 'gmina wiejska'),(3, 'gmina miejsko-wiejska'),(4, 'miasto w gminie miejsko-wiejskiej'),(5, 'obszar wiejski w gminie miejsko-wiejskiej'),(8, 'dzielnice '),(9, 'delegatura')`);
}

function loadTerc(db) {
  const { rows } = parseCsv(CSV_FILES.terc);
  const stmt = db.prepare(
    "INSERT INTO terc (woj, pow, gmi, rodz, nazwa, nazwa_dod) VALUES (?, ?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.WOJ, 10),
        r.POW ? parseInt(r.POW, 10) : null,
        r.GMI ? parseInt(r.GMI, 10) : null,
        r.RODZ ? parseInt(r.RODZ, 10) : null,
        r.NAZWA,
        r.NAZWA_DOD || null
      );
    }
  });
  tx(rows);
  return rows.length;
}

function loadSimc(db) {
  const { rows } = parseCsv(CSV_FILES.simc);
  const stmt = db.prepare(
    "INSERT INTO simc (woj, pow, gmi, rodz_gmi, rm, nazwa, sym, sympod) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.WOJ, 10),
        parseInt(r.POW, 10),
        parseInt(r.GMI, 10),
        parseInt(r.RODZ_GMI, 10),
        parseInt(r.RM, 10),
        r.NAZWA,
        parseInt(r.SYM, 10),
        parseInt(r.SYMPOD, 10),
      );
    }
  });
  tx(rows);
  return rows.length;
}

function loadUlic(db) {
  const { rows } = parseCsv(CSV_FILES.ulic);
  const stmt = db.prepare(
    "INSERT INTO ulic (sym, sym_ul, cecha, nazwa_1, nazwa_2) VALUES (?, ?, ?, ?, ?)"
  );
  const tx = db.transaction((rows) => {
    for (const r of rows) {
      stmt.run(
        parseInt(r.SYM, 10),
        parseInt(r.SYM_UL, 10),
        r.CECHA || null,
        r.NAZWA_1,
        r.NAZWA_2 || null
      );
    }
  });
  tx(rows);
  return rows.length;
}

function fixData(db) {
  db.exec(`update simc as osiedle
            set sympod = miasto.sym
            from simc as miasto 
            where miasto.woj = osiedle.woj 
            and miasto.pow = osiedle.pow
            and miasto.rodz_gmi not in (8, 9)
            and osiedle.rodz_gmi in (8, 9)
            and osiedle.rm <> 99`);
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

console.log("Loading dictionaries...");
loadDictionaries(db);
console.log("  → OK");

console.log("Loading TERC...");
const tercCount = loadTerc(db);
console.log(`  → ${tercCount} rows`);

console.log("Loading SIMC...");
const simcCount = loadSimc(db);
console.log(`  → ${simcCount} rows`);

console.log("Loading ULIC...");
const ulicCount = loadUlic(db);
console.log(`  → ${ulicCount} rows`);

console.log("Fixing data...");
fixData(db);
console.log(`  → OK`);

db.pragma("journal_mode = DELETE");
db.close();
console.log(`Done. Database saved to ${DB_PATH}`);
