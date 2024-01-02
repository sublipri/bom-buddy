SELECT 
	radar_id,
	feature,
	radar_type_id,
	image,
	filename
FROM
	radar_feature_layer
WHERE
	radar_id = (?)
AND
	radar_type_id = (?);
