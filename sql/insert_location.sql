INSERT INTO location (
	id,
	geohash,
	station_id,
	has_wave,
	latitude,
	longitude,
	marine_area_id,
	name,
	state,
	postcode,
	tidal_point,
	timezone,
	weather)
VALUES (
	:id,
	:geohash,
	:station_id,
	:has_wave,
	:latitude,
	:longitude,
	:marine_area_id,
	:name,
	:state,
	:postcode,
	:tidal_point,
	:timezone,
	:weather
);
