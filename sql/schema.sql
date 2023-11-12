CREATE TABLE IF NOT EXISTS station (
	id INT PRIMARY KEY,
	district_id TEXT NOT NULL,
	name TEXT NOT NULL,
	start INT NOT NULL,
	end INT,
	latitude NUMERIC(4,4) NOT NULL,
	longitude NUMERIC(4,4) NOT NULL,
	source TEXT,
	state TEXT NOT NULL,
	height NUMERIC(4,1),
	barometric_height NUMERIC(4,1),
	wmo_id INT
);

CREATE TABLE IF NOT EXISTS location (
    id TEXT PRIMARY KEY,
    geohash TEXT NOT NULL,
    station_id INTEGER NOT NULL,
    has_wave INTEGER NOT NULL,
    latitude NUMERIC(4,14) NOT NULL,
    longitude NUMERIC(4,14) NOT NULL,
    marine_area_id TEXT,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
	postcode TEXT NOT NULL,
    tidal_point TEXT,
    timezone TEXT NOT NULL,
    weather TEXT NOT NULL,
	FOREIGN KEY(station_id) REFERENCES station(id)
);
