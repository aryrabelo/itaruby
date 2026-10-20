class Http
  def fetch
    Faraday.get("https://example.test")
  end
end
