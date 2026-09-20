# A receiver that is not a project class never reaches the class-object
# arms, so the census must record nothing — the historical SecureRandom/Kernel
# false-positive population is structurally out of the bucket.
SecureRandom.base64
