/* Generated from providers/catalog.json. Do not edit. */
(function (root, factory) {
    const catalog = factory();
    if (typeof module === 'object' && module.exports) module.exports = catalog;
    else root.AwariaProviderCatalog = catalog;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    'use strict';
    const providers = [
    {
        "id": "enea",
        "utility": "power",
        "category": "power",
        "defaultNotify": true,
        "frontendOrder": 0,
        "label": "Enea",
        "names": {
            "pl": "Enea",
            "en": "Enea"
        },
        "shortNames": {
            "pl": "Enea",
            "en": "Enea"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Enea"
            },
            "android": {
                "pl": "Enea",
                "en": "Enea"
            }
        }
    },
    {
        "id": "energa",
        "utility": "power",
        "category": "power",
        "defaultNotify": true,
        "frontendOrder": 1,
        "label": "Energa",
        "names": {
            "pl": "Energa",
            "en": "Energa"
        },
        "shortNames": {
            "pl": "Energa",
            "en": "Energa"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Energa"
            },
            "android": {
                "pl": "Energa",
                "en": "Energa"
            }
        }
    },
    {
        "id": "pge",
        "utility": "power",
        "category": "power",
        "defaultNotify": true,
        "frontendOrder": 2,
        "label": "PGE",
        "names": {
            "pl": "PGE",
            "en": "PGE"
        },
        "shortNames": {
            "pl": "PGE",
            "en": "PGE"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PGE"
            },
            "android": {
                "pl": "PGE",
                "en": "PGE"
            }
        }
    },
    {
        "id": "stoen",
        "utility": "power",
        "category": "power",
        "defaultNotify": true,
        "frontendOrder": 3,
        "label": "Stoen",
        "names": {
            "pl": "Stoen",
            "en": "Stoen"
        },
        "shortNames": {
            "pl": "Stoen",
            "en": "Stoen"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Stoen"
            },
            "android": {
                "pl": "Stoen",
                "en": "Stoen"
            }
        },
        "region": "warszawa"
    },
    {
        "id": "tauron",
        "utility": "power",
        "category": "power",
        "defaultNotify": true,
        "frontendOrder": 4,
        "label": "Tauron",
        "names": {
            "pl": "Tauron",
            "en": "Tauron"
        },
        "shortNames": {
            "pl": "Tauron",
            "en": "Tauron"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Tauron"
            },
            "android": {
                "pl": "Tauron",
                "en": "Tauron"
            }
        }
    },
    {
        "id": "psg",
        "utility": "gas",
        "category": "gas",
        "defaultNotify": true,
        "frontendOrder": 5,
        "label": "PSG",
        "names": {
            "pl": "PSG",
            "en": "PSG"
        },
        "shortNames": {
            "pl": "PSG",
            "en": "PSG"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PSG"
            },
            "android": {
                "pl": "PSG",
                "en": "PSG"
            }
        }
    },
    {
        "id": "eco_sa",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 6,
        "label": "ECO SA",
        "names": {
            "pl": "ECO SA",
            "en": "ECO SA"
        },
        "shortNames": {
            "pl": "ECO SA",
            "en": "ECO SA"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ECO SA"
            },
            "android": {
                "pl": "ECO SA",
                "en": "ECO SA"
            }
        }
    },
    {
        "id": "enea_cieplo",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 7,
        "label": "ENEA Ciepło Białystok",
        "names": {
            "pl": "ENEA Ciepło Białystok",
            "en": "ENEA Ciepło Białystok"
        },
        "shortNames": {
            "pl": "ENEA Ciepło",
            "en": "ENEA Ciepło"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ENEA Ciepło Białystok"
            },
            "android": {
                "pl": "ENEA Ciepło",
                "en": "ENEA Ciepło"
            }
        },
        "region": "bialystok"
    },
    {
        "id": "energa_cieplo_kaliskie",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 8,
        "label": "Energa Ciepło Kaliskie",
        "names": {
            "pl": "Energa Ciepło Kaliskie",
            "en": "Energa Ciepło Kaliskie"
        },
        "shortNames": {
            "pl": "Energa Kalisz",
            "en": "Energa Kalisz"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Energa Ciepło Kaliskie"
            },
            "android": {
                "pl": "Energa Ciepło Kalisz",
                "en": "Energa Ciepło Kalisz"
            }
        },
        "region": "kalisz"
    },
    {
        "id": "epec_elblag",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 9,
        "label": "EPEC Elbląg",
        "names": {
            "pl": "EPEC Elbląg",
            "en": "EPEC Elbląg"
        },
        "shortNames": {
            "pl": "EPEC Elbląg",
            "en": "EPEC Elbląg"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "EPEC Elbląg"
            },
            "android": {
                "pl": "EPEC Elbląg",
                "en": "EPEC Elbląg"
            }
        },
        "region": "elblag"
    },
    {
        "id": "fortum",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 10,
        "label": "Fortum",
        "names": {
            "pl": "Fortum",
            "en": "Fortum"
        },
        "shortNames": {
            "pl": "Fortum",
            "en": "Fortum"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Fortum"
            },
            "android": {
                "pl": "Fortum",
                "en": "Fortum"
            }
        }
    },
    {
        "id": "gpec",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 11,
        "label": "GPEC Gdańsk",
        "names": {
            "pl": "GPEC Gdańsk",
            "en": "GPEC Gdańsk"
        },
        "shortNames": {
            "pl": "GPEC",
            "en": "GPEC"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "GPEC Gdańsk"
            },
            "android": {
                "pl": "GPEC GDA",
                "en": "GPEC GDA"
            }
        },
        "region": "gdansk"
    },
    {
        "id": "kpec_bydgoszcz",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 12,
        "label": "KPEC Bydgoszcz",
        "names": {
            "pl": "KPEC Bydgoszcz",
            "en": "KPEC Bydgoszcz"
        },
        "shortNames": {
            "pl": "KPEC",
            "en": "KPEC"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "KPEC Bydgoszcz"
            },
            "android": {
                "pl": "KPEC BDG",
                "en": "KPEC BDG"
            }
        },
        "region": "bydgoszcz"
    },
    {
        "id": "lpec",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 13,
        "label": "LPEC Lublin",
        "names": {
            "pl": "LPEC Lublin",
            "en": "LPEC Lublin"
        },
        "shortNames": {
            "pl": "LPEC",
            "en": "LPEC"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "LPEC Lublin"
            },
            "android": {
                "pl": "LPEC",
                "en": "LPEC"
            }
        },
        "region": "lublin"
    },
    {
        "id": "mec_koszalin",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 14,
        "label": "MEC Koszalin",
        "names": {
            "pl": "MEC Koszalin",
            "en": "MEC Koszalin"
        },
        "shortNames": {
            "pl": "MEC Koszalin",
            "en": "MEC Koszalin"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MEC Koszalin"
            },
            "android": {
                "pl": "MEC Koszalin",
                "en": "MEC Koszalin"
            }
        },
        "region": "mec_koszalin_area"
    },
    {
        "id": "mpec_kielce",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 15,
        "label": "MPEC Kielce",
        "names": {
            "pl": "MPEC Kielce",
            "en": "MPEC Kielce"
        },
        "shortNames": {
            "pl": "MPEC KIE",
            "en": "MPEC KIE"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPEC Kielce"
            },
            "android": {
                "pl": "MPEC Kielce",
                "en": "MPEC Kielce"
            }
        },
        "region": "kielce"
    },
    {
        "id": "mpec_rzeszow",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 16,
        "label": "MPEC Rzeszów",
        "names": {
            "pl": "MPEC Rzeszów",
            "en": "MPEC Rzeszów"
        },
        "shortNames": {
            "pl": "MPEC RZE",
            "en": "MPEC RZE"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPEC Rzeszów"
            },
            "android": {
                "pl": "MPEC Rzeszów",
                "en": "MPEC Rzeszów"
            }
        },
        "region": "rzeszow"
    },
    {
        "id": "opec_gdynia",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 17,
        "label": "OPEC Gdynia",
        "names": {
            "pl": "OPEC Gdynia",
            "en": "OPEC Gdynia"
        },
        "shortNames": {
            "pl": "OPEC",
            "en": "OPEC"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "OPEC Gdynia"
            },
            "android": {
                "pl": "OPEC GDY",
                "en": "OPEC GDY"
            }
        },
        "region": "opec_gdynia_area"
    },
    {
        "id": "pec_gliwice",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 18,
        "label": "PEC Gliwice",
        "names": {
            "pl": "PEC Gliwice",
            "en": "PEC Gliwice"
        },
        "shortNames": {
            "pl": "PEC Gliwice",
            "en": "PEC Gliwice"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PEC Gliwice"
            },
            "android": {
                "pl": "PEC Gliwice",
                "en": "PEC Gliwice"
            }
        },
        "region": "pec_gliwice_area"
    },
    {
        "id": "pge_torun_heat",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 19,
        "label": "PGE Toruń",
        "names": {
            "pl": "PGE Toruń",
            "en": "PGE Toruń"
        },
        "shortNames": {
            "pl": "PGE Toruń",
            "en": "PGE Toruń"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PGE Toruń"
            },
            "android": {
                "pl": "PGE Toruń",
                "en": "PGE Toruń"
            }
        },
        "region": "torun"
    },
    {
        "id": "sec",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 20,
        "label": "SEC Szczecin",
        "names": {
            "pl": "SEC Szczecin",
            "en": "SEC Szczecin"
        },
        "shortNames": {
            "pl": "SEC",
            "en": "SEC"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "SEC Szczecin"
            },
            "android": {
                "pl": "SEC",
                "en": "SEC"
            }
        },
        "region": "szczecin"
    },
    {
        "id": "tauron_heat",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 21,
        "label": "Tauron Ciepło",
        "names": {
            "pl": "Tauron Ciepło",
            "en": "Tauron Ciepło"
        },
        "shortNames": {
            "pl": "Tauron Ciepło",
            "en": "Tauron Heat"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Tauron Ciepło"
            },
            "android": {
                "pl": "Tauron Ciepło",
                "en": "Tauron Ciepło"
            }
        }
    },
    {
        "id": "therma_bielsko",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 22,
        "label": "Therma Bielsko",
        "names": {
            "pl": "Therma Bielsko",
            "en": "Therma Bielsko"
        },
        "shortNames": {
            "pl": "Therma Bielsko",
            "en": "Therma Bielsko"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Therma Bielsko"
            },
            "android": {
                "pl": "Therma Bielsko",
                "en": "Therma Bielsko"
            }
        },
        "region": "therma_bielsko"
    },
    {
        "id": "veolia_lodz",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 23,
        "label": "Veolia Łódź",
        "names": {
            "pl": "Veolia Łódź",
            "en": "Veolia Łódź"
        },
        "shortNames": {
            "pl": "Veolia Łódź",
            "en": "Veolia Łódź"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Veolia Łódź"
            },
            "android": {
                "pl": "Veolia LDZ",
                "en": "Veolia LDZ"
            }
        },
        "region": "lodz"
    },
    {
        "id": "veolia_poznan",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 24,
        "label": "Veolia Poznań",
        "names": {
            "pl": "Veolia Poznań",
            "en": "Veolia Poznań"
        },
        "shortNames": {
            "pl": "Veolia Poznań",
            "en": "Veolia Poznań"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Veolia Poznań"
            },
            "android": {
                "pl": "Veolia POZ",
                "en": "Veolia POZ"
            }
        },
        "region": "poznan"
    },
    {
        "id": "veolia_warszawa",
        "utility": "heating",
        "category": "heating",
        "defaultNotify": true,
        "frontendOrder": 25,
        "label": "Veolia Warszawa",
        "names": {
            "pl": "Veolia Warszawa",
            "en": "Veolia Warszawa"
        },
        "shortNames": {
            "pl": "Veolia Warszawa",
            "en": "Veolia Warszawa"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Veolia Warszawa"
            },
            "android": {
                "pl": "Veolia WAW",
                "en": "Veolia WAW"
            }
        },
        "region": "warszawa"
    },
    {
        "id": "aqua_bielsko",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 26,
        "label": "AQUA S.A.",
        "names": {
            "pl": "AQUA S.A.",
            "en": "AQUA S.A."
        },
        "shortNames": {
            "pl": "AQUA S.A.",
            "en": "AQUA S.A."
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "AQUA S.A."
            },
            "android": {
                "pl": "AQUA Bielsko-Biała",
                "en": "AQUA Bielsko-Biała"
            }
        },
        "region": "aqua_bielsko_area"
    },
    {
        "id": "aquanet",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 27,
        "label": "Aquanet",
        "names": {
            "pl": "Aquanet",
            "en": "Aquanet"
        },
        "shortNames": {
            "pl": "Aquanet",
            "en": "Aquanet"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Aquanet"
            },
            "android": {
                "pl": "Aquanet",
                "en": "Aquanet"
            }
        },
        "region": "poznan"
    },
    {
        "id": "bytomskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 28,
        "label": "Bytomskie Wodociągi",
        "names": {
            "pl": "Bytomskie Wodociągi",
            "en": "Bytomskie Wodociągi"
        },
        "shortNames": {
            "pl": "BW Bytom",
            "en": "BW Bytom"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Bytomskie Wodociągi"
            },
            "android": {
                "pl": "Bytomskie Wodociągi",
                "en": "Bytom Water Utility"
            }
        },
        "region": "bytom"
    },
    {
        "id": "chspwik",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 29,
        "label": "ChŚPWiK",
        "names": {
            "pl": "ChŚPWiK",
            "en": "ChŚPWiK"
        },
        "shortNames": {
            "pl": "ChŚPWiK",
            "en": "ChŚPWiK"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ChŚPWiK"
            },
            "android": {
                "pl": "ChŚPWiK",
                "en": "ChŚPWiK"
            }
        },
        "region": "chspwik_area"
    },
    {
        "id": "dabrowskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 30,
        "label": "Dąbrowskie Wodociągi",
        "names": {
            "pl": "Dąbrowskie Wodociągi",
            "en": "Dąbrowskie Wodociągi"
        },
        "shortNames": {
            "pl": "DW Dąbrowa",
            "en": "DW Dąbrowa"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Dąbrowskie Wodociągi"
            },
            "android": {
                "pl": "Dąbrowskie Wodociągi",
                "en": "Dąbrowskie Waterworks"
            }
        },
        "region": "dabrowskie_wodociagi_area"
    },
    {
        "id": "gdanskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 31,
        "label": "Gdańskie Wodociągi",
        "names": {
            "pl": "Gdańskie Wodociągi",
            "en": "Gdańskie Wodociągi"
        },
        "shortNames": {
            "pl": "Gdańsk Woda",
            "en": "Gdańsk Water"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Gdańskie Wodociągi"
            },
            "android": {
                "pl": "GW GDA",
                "en": "GW GDA"
            }
        },
        "region": "gdansk"
    },
    {
        "id": "katowickie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 32,
        "label": "Katowickie Wodociągi",
        "names": {
            "pl": "Katowickie Wodociągi",
            "en": "Katowickie Wodociągi"
        },
        "shortNames": {
            "pl": "Kat. Wodociągi",
            "en": "Kat. Wodociągi"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Katowickie Wodociągi"
            },
            "android": {
                "pl": "KW KTW",
                "en": "KW KTW"
            }
        },
        "region": "katowice"
    },
    {
        "id": "lpwik_legnica",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 33,
        "label": "LPWiK Legnica",
        "names": {
            "pl": "LPWiK Legnica",
            "en": "LPWiK Legnica"
        },
        "shortNames": {
            "pl": "LPWiK",
            "en": "LPWiK"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "LPWiK Legnica"
            },
            "android": {
                "pl": "LPWiK Legnica",
                "en": "LPWiK Legnica"
            }
        },
        "region": "lpwik_legnica_area"
    },
    {
        "id": "mpwik_lublin",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 34,
        "label": "MPWiK Lublin",
        "names": {
            "pl": "MPWiK Lublin",
            "en": "MPWiK Lublin"
        },
        "shortNames": {
            "pl": "MPWiK LUB",
            "en": "MPWiK LUB"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPWiK Lublin"
            },
            "android": {
                "pl": "MPWiK Lublin",
                "en": "MPWiK LBL"
            }
        },
        "region": "lublin"
    },
    {
        "id": "mpwik_rzeszow",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 35,
        "label": "MPWiK Rzeszów",
        "names": {
            "pl": "MPWiK Rzeszów",
            "en": "MPWiK Rzeszów"
        },
        "shortNames": {
            "pl": "MPWiK RZE",
            "en": "MPWiK RZE"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPWiK Rzeszów"
            },
            "android": {
                "pl": "MPWiK Rzeszów",
                "en": "MPWiK RZE"
            }
        },
        "region": "mpwik_rzeszow_area"
    },
    {
        "id": "mpwik_warszawa",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 36,
        "label": "MPWiK Warszawa",
        "names": {
            "pl": "MPWiK Warszawa",
            "en": "MPWiK Warszawa"
        },
        "shortNames": {
            "pl": "MPWiK WAW",
            "en": "MPWiK WAW"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPWiK Warszawa"
            },
            "android": {
                "pl": "MPWiK WAW",
                "en": "MPWiK WAW"
            }
        },
        "region": "mpwik_warszawa_area"
    },
    {
        "id": "mpwik_wroclaw",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 37,
        "label": "MPWiK Wrocław",
        "names": {
            "pl": "MPWiK Wrocław",
            "en": "MPWiK Wrocław"
        },
        "shortNames": {
            "pl": "MPWiK WRO",
            "en": "MPWiK WRO"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MPWiK Wrocław"
            },
            "android": {
                "pl": "MPWiK WRO",
                "en": "MPWiK WRO"
            }
        },
        "region": "wroclaw"
    },
    {
        "id": "mwik_bydgoszcz",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 38,
        "label": "MWiK Bydgoszcz",
        "names": {
            "pl": "MWiK Bydgoszcz",
            "en": "MWiK Bydgoszcz"
        },
        "shortNames": {
            "pl": "MWiK BDG",
            "en": "MWiK BDG"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MWiK Bydgoszcz"
            },
            "android": {
                "pl": "MWiK BDG",
                "en": "MWiK BDG"
            }
        },
        "region": "bydgoszcz"
    },
    {
        "id": "mwik_koszalin",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 39,
        "label": "MWiK Koszalin",
        "names": {
            "pl": "MWiK Koszalin",
            "en": "MWiK Koszalin"
        },
        "shortNames": {
            "pl": "MWiK Koszalin",
            "en": "MWiK Koszalin"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "MWiK Koszalin"
            },
            "android": {
                "pl": "MWiK Koszalin",
                "en": "MWiK Koszalin"
            }
        },
        "region": "koszalin"
    },
    {
        "id": "pewik_gdynia",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 40,
        "label": "PEWIK Gdynia",
        "names": {
            "pl": "PEWIK Gdynia",
            "en": "PEWIK Gdynia"
        },
        "shortNames": {
            "pl": "PEWIK GDY",
            "en": "PEWIK GDY"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PEWIK Gdynia"
            },
            "android": {
                "pl": "PEWIK GDY",
                "en": "PEWIK GDY"
            }
        },
        "region": "pewik_gdynia_area"
    },
    {
        "id": "puk_rokietnica",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 41,
        "label": "PUK Rokietnica",
        "names": {
            "pl": "PUK Rokietnica",
            "en": "PUK Rokietnica"
        },
        "shortNames": {
            "pl": "PUK Rokietnica",
            "en": "PUK Rokietnica"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PUK Rokietnica"
            },
            "android": {
                "pl": "PUK Rokiet.",
                "en": "PUK Rokietnica"
            }
        },
        "region": "rokietnica"
    },
    {
        "id": "pwik_czestochowa",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 42,
        "label": "PWiK Częstochowa",
        "names": {
            "pl": "PWiK Częstochowa",
            "en": "PWiK Częstochowa"
        },
        "shortNames": {
            "pl": "PWiK Częstochowa",
            "en": "PWiK Częstochowa"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Częstochowa"
            },
            "android": {
                "pl": "PWiK CZW",
                "en": "PWiK CZW"
            }
        },
        "region": "czestochowa"
    },
    {
        "id": "pwik_gliwice",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 43,
        "label": "PWiK Gliwice",
        "names": {
            "pl": "PWiK Gliwice",
            "en": "PWiK Gliwice"
        },
        "shortNames": {
            "pl": "PWiK Gliwice",
            "en": "PWiK Gliwice"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Gliwice"
            },
            "android": {
                "pl": "PWiK Gliwice",
                "en": "PWiK Gliwice"
            }
        },
        "region": "pwik_gliwice_area"
    },
    {
        "id": "pwik_gorzow",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 44,
        "label": "PWiK Gorzów",
        "names": {
            "pl": "PWiK Gorzów",
            "en": "PWiK Gorzów"
        },
        "shortNames": {
            "pl": "PWiK Gorzów",
            "en": "PWiK Gorzów"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Gorzów Wielkopolski"
            },
            "android": {
                "pl": "PWiK Gorzów Wielkopolski",
                "en": "PWiK Gorzów Wielkopolski"
            }
        },
        "region": "pwik_gorzow_area"
    },
    {
        "id": "pwik_kalisz",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 45,
        "label": "PWiK Kalisz",
        "names": {
            "pl": "PWiK Kalisz",
            "en": "PWiK Kalisz"
        },
        "shortNames": {
            "pl": "PWiK Kalisz",
            "en": "PWiK Kalisz"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Kalisz"
            },
            "android": {
                "pl": "PWiK Kalisz",
                "en": "PWiK Kalisz"
            }
        },
        "region": "kalisz"
    },
    {
        "id": "pwik_olsztyn",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 46,
        "label": "PWiK Olsztyn",
        "names": {
            "pl": "PWiK Olsztyn",
            "en": "PWiK Olsztyn"
        },
        "shortNames": {
            "pl": "PWiK Olsztyn",
            "en": "PWiK Olsztyn"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Olsztyn"
            },
            "android": {
                "pl": "PWiK Olsztyn",
                "en": "PWiK Olsztyn"
            }
        },
        "region": "pwik_olsztyn_area"
    },
    {
        "id": "pwik_ruda_slaska",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 47,
        "label": "PWiK Ruda Śląska",
        "names": {
            "pl": "PWiK Ruda Śląska",
            "en": "PWiK Ruda Śląska"
        },
        "shortNames": {
            "pl": "PWiK Ruda Śl.",
            "en": "PWiK Ruda Śl."
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Ruda Śląska"
            },
            "android": {
                "pl": "PWiK Ruda Śląska",
                "en": "PWiK Ruda Śląska"
            }
        },
        "region": "ruda_slaska"
    },
    {
        "id": "pwik_rybnik",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 48,
        "label": "PWiK Rybnik",
        "names": {
            "pl": "PWiK Rybnik",
            "en": "PWiK Rybnik"
        },
        "shortNames": {
            "pl": "PWiK Rybnik",
            "en": "PWiK Rybnik"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "PWiK Rybnik"
            },
            "android": {
                "pl": "PWiK Rybnik",
                "en": "PWiK Rybnik"
            }
        },
        "region": "pwik_rybnik_area"
    },
    {
        "id": "rpwik_tychy",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 49,
        "label": "RPWiK Tychy",
        "names": {
            "pl": "RPWiK Tychy",
            "en": "RPWiK Tychy"
        },
        "shortNames": {
            "pl": "RPWiK Tychy",
            "en": "RPWiK Tychy"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "RPWiK Tychy"
            },
            "android": {
                "pl": "RPWiK Tychy",
                "en": "RPWiK Tychy"
            }
        },
        "region": "rpwik_tychy_area"
    },
    {
        "id": "sosnowieckie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 50,
        "label": "Sosnowieckie Wodociągi",
        "names": {
            "pl": "Sosnowieckie Wodociągi",
            "en": "Sosnowieckie Wodociągi"
        },
        "shortNames": {
            "pl": "Wodociągi Sosnowiec",
            "en": "Sosnowiec Water"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Sosnowieckie Wodociągi"
            },
            "android": {
                "pl": "Sosnowieckie Wodociągi",
                "en": "Sosnowieckie Wodociągi"
            }
        },
        "region": "sosnowiec"
    },
    {
        "id": "tarnowskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 51,
        "label": "Tarnowskie Wodociągi",
        "names": {
            "pl": "Tarnowskie Wodociągi",
            "en": "Tarnowskie Wodociągi"
        },
        "shortNames": {
            "pl": "TW Tarnów",
            "en": "TW Tarnów"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Tarnowskie Wodociągi"
            },
            "android": {
                "pl": "Tarnowskie Wodociągi",
                "en": "Tarnowskie Wodociągi"
            }
        },
        "region": "tarnowskie_wodociagi_area"
    },
    {
        "id": "torunskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 52,
        "label": "Toruńskie Wodociągi",
        "names": {
            "pl": "Toruńskie Wodociągi",
            "en": "Toruńskie Wodociągi"
        },
        "shortNames": {
            "pl": "Toruńskie Wodociągi",
            "en": "Toruńskie Wodociągi"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Toruńskie Wodociągi"
            },
            "android": {
                "pl": "TW Toruń",
                "en": "TW Toruń"
            }
        },
        "region": "torunskie_wodociagi_area"
    },
    {
        "id": "wik_opole",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 53,
        "label": "WiK Opole",
        "names": {
            "pl": "WiK Opole",
            "en": "WiK Opole"
        },
        "shortNames": {
            "pl": "WiK Opole",
            "en": "WiK Opole"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "WiK Opole"
            },
            "android": {
                "pl": "WiK Opole",
                "en": "WiK Opole"
            }
        },
        "region": "wik_opole_area"
    },
    {
        "id": "wm_radom",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 54,
        "label": "WM Radom",
        "names": {
            "pl": "WM Radom",
            "en": "WM Radom"
        },
        "shortNames": {
            "pl": "WM Radom",
            "en": "WM Radom"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "WM Radom"
            },
            "android": {
                "pl": "WM Radom",
                "en": "WM Radom"
            }
        },
        "region": "wm_radom_area"
    },
    {
        "id": "wmk",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 55,
        "label": "WMK Kraków",
        "names": {
            "pl": "WMK Kraków",
            "en": "WMK Kraków"
        },
        "shortNames": {
            "pl": "WMK",
            "en": "WMK"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "WMK Kraków"
            },
            "android": {
                "pl": "WMK",
                "en": "WMK"
            }
        },
        "region": "krakow"
    },
    {
        "id": "wodociagi_bialostockie",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 56,
        "label": "Wodociągi Białostockie",
        "names": {
            "pl": "Wodociągi Białostockie",
            "en": "Wodociągi Białostockie"
        },
        "shortNames": {
            "pl": "WOBI Białystok",
            "en": "WOBI Białystok"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Wodociągi Białostockie"
            },
            "android": {
                "pl": "WOBI BST",
                "en": "WOBI BST"
            }
        },
        "region": "wodociagi_bialostockie_area"
    },
    {
        "id": "wodociagi_kieleckie",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 57,
        "label": "Wodociągi Kieleckie",
        "names": {
            "pl": "Wodociągi Kieleckie",
            "en": "Wodociągi Kieleckie"
        },
        "shortNames": {
            "pl": "WK Kielce",
            "en": "WK Kielce"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Wodociągi Kieleckie"
            },
            "android": {
                "pl": "WK Kielce",
                "en": "WK Kielce"
            }
        },
        "region": "wodociagi_kieleckie_area"
    },
    {
        "id": "wodociagi_plockie",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 58,
        "label": "Wodociągi Płockie",
        "names": {
            "pl": "Wodociągi Płockie",
            "en": "Wodociągi Płockie"
        },
        "shortNames": {
            "pl": "Wodociągi Płockie",
            "en": "Wodociągi Płockie"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "Wodociągi Płockie"
            },
            "android": {
                "pl": "WP Płock",
                "en": "WP Płock"
            }
        },
        "region": "plock"
    },
    {
        "id": "wpwik_walbrzych",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 59,
        "label": "WPWiK Wałbrzych",
        "names": {
            "pl": "WPWiK Wałbrzych",
            "en": "WPWiK Wałbrzych"
        },
        "shortNames": {
            "pl": "WPWiK Wałbrzych",
            "en": "WPWiK Wałbrzych"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "WPWiK Wałbrzych"
            },
            "android": {
                "pl": "WPWiK Wałbrzych",
                "en": "WPWiK Wałbrzych"
            }
        },
        "region": "wpwik_walbrzych_area"
    },
    {
        "id": "zabrzanskie_wodociagi",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 60,
        "label": "ZPWiK Zabrze",
        "names": {
            "pl": "ZPWiK Zabrze",
            "en": "ZPWiK Zabrze"
        },
        "shortNames": {
            "pl": "ZPWiK Zabrze",
            "en": "ZPWiK Zabrze"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ZPWiK Zabrze"
            },
            "android": {
                "pl": "ZPWiK Zabrze",
                "en": "ZPWiK Zabrze"
            }
        },
        "region": "zabrze"
    },
    {
        "id": "zwik_lodz",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 61,
        "label": "ZWIK Łódź",
        "names": {
            "pl": "ZWIK Łódź",
            "en": "ZWiK Łódź"
        },
        "shortNames": {
            "pl": "ZWIK Łódź",
            "en": "ZWiK Łódź"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ZWiK Łódź"
            },
            "android": {
                "pl": "ZWIK Łódź",
                "en": "ZWIK Łódź"
            }
        },
        "region": "lodz"
    },
    {
        "id": "zwik_zielona_gora",
        "utility": "water",
        "category": "water",
        "defaultNotify": true,
        "frontendOrder": 62,
        "label": "ZWiK Zielona Góra",
        "names": {
            "pl": "ZWiK Zielona Góra",
            "en": "ZWiK Zielona Góra"
        },
        "shortNames": {
            "pl": "ZWiK Zielona Góra",
            "en": "ZWiK Zielona Góra"
        },
        "aliases": [],
        "surfaceNames": {
            "rust": {
                "pl": "ZWiK Zielona Góra"
            },
            "android": {
                "pl": "ZWiK ZG",
                "en": "ZWiK ZG"
            }
        },
        "region": "zwik_zielona_gora_area"
    }
].map((provider) => Object.freeze(provider));
    const byId = Object.freeze(Object.fromEntries(providers.map((provider) => [provider.id, provider])));
    function getName(id, locale = 'pl', short = false, surface = 'frontend') {
        const provider = byId[id];
        if (!provider) return null;
        const names = surface === 'frontend'
            ? (short ? provider.shortNames : provider.names)
            : provider.surfaceNames?.[surface];
        return names?.[locale] || names?.pl || names?.en || null;
    }
    return Object.freeze({ providers: Object.freeze(providers), byId, getName });
});
