INSERT INTO radar_data_layer (
	image,
	radar_id,
	radar_type_id,
	timestamp,
	filename)
VALUES (
	:image,
	:radar_id,
	:radar_type_id,
	:timestamp,
	:filename
)
