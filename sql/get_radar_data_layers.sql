SELECT 
	radar_id, 
	radar_type_id, 
	image, 
	timestamp,
	filename
FROM 
	radar_data_layer 
WHERE 
	radar_id = (?)
AND
	radar_type_id = (?)
ORDER BY 
	timestamp DESC
LIMIT (?);
