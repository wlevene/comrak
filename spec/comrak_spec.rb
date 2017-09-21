require 'comrak'

describe "Comrak" do
  it "can do a thing" do
    expect(Comrak.markdown_to_html("Hello, _world_!")).to eq("<p>Hello, <em>world</em>!</p>\n")
  end
end
