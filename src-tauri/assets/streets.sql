SELECT street.cecha || ifnull(' ' || street.nazwa_2, '') || ' ' || street.nazwa_1 as full_street_name,
    street.sym city_id,
    street.sym_ul street_id
FROM simc city
    LEFT JOIN simc city_part ON city.sym = city_part.sympod
    LEFT JOIN ulic street ON city.sym = street.sym
    OR city_part.sym = street.sym
WHERE city.sym = city.sympod
    AND street.sym_ul is not null
    AND city.sym = 969400
    AND nazwa_1 like '%Kuźnicza%';