SELECT voivodeship.nazwa voivodeship,
    district.nazwa district,
    commune.nazwa commune,
    city.nazwa city,
    city.sym city_id
FROM simc city
    LEFT JOIN terc voivodeship ON city.woj = voivodeship.woj
    AND voivodeship.pow is null
    AND voivodeship.gmi is null
    LEFT JOIN terc district ON city.woj = district.woj
    AND city.pow = district.pow
    AND district.gmi is null
    LEFT JOIN terc commune ON city.woj = commune.woj
    AND city.pow = commune.pow
    AND city.gmi = commune.gmi
    AND city.rodz_gmi = commune.rodz
WHERE city.sym = city.sympod
    AND city.nazwa = 'Poznań';