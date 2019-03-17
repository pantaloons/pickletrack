# Pickletrack
Pickletrack is a service which finds the nearest bar serving picklebacks in New York City.

Pickletrack has two main components, a scraper and a web server.

## Scraper
The scraper accesses the Foursquare API to build a database of bars and their comments mentioning the phrase "pickleback". The scraper is run independently of the web server, and writes output to a JSON file under static/data/YYYYMMDD.json, it then updates a symlink to this file at static/data/current.json.

## Server
The web server reloads the list of bars every day and then uses this information, along with a users location to determine nearby bars to suggest.

## Building
`cargo build`

## Deploying
`./deploy/aws/provision.sh` \
You can provision an AWS instance to serve Pickletrack by running this command on the instance. Note that Pickletrack must be served behind SSL for the location API to work. Pickletrack does not temrinate SSL and this should be done before forwarding it requests.

`./deploy/deploy-to-aws.sh` \
The deploy-to-aws command will copy necessary files onto the instance and then start the server.