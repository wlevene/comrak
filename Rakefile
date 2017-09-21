require 'bundler/setup'
require 'helix_runtime/build_task'
require 'rspec/core/rake_task'

$stdout.sync = true

HelixRuntime::BuildTask.new do |t|
end

RSpec::Core::RakeTask.new(:spec) do |t|
  t.verbose = false
end

task :spec => :build
task :default => :spec
