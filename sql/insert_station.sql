INSERT INTO station (
	id,
	district_id,
	name,
	start,
	end,
	latitude,
	longitude,
	source,
	state,
	height,
	barometric_height,
	wmo_id)
 VALUES (
	:id,
	:district_id,
	:name,
	:start,
	:end,
	:latitude,
	:longitude,
	:source,
	:state,
	:height,
	:barometric_height,
	:wmo_id
);
