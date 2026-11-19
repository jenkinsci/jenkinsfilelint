import groovy.transform.Field

@Field def BRANCH = 'main'

@NonCPS
def parse(String text) {
    return text.split('@main')
}

@Grab('org.yaml:snakeyaml@main')
def yaml
