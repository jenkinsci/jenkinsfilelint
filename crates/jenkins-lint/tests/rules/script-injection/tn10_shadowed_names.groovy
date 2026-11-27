// Parameters and variables of the file named like the build's are not the
// build's: a shared library step's Map params, a method parameter named
// CHANGE_TITLE, a local env.
def call(Map params = [:]) {
    sh "deploy --target ${params.target}"
}

def notify(String CHANGE_TITLE) {
    sh "echo ${CHANGE_TITLE}"
}

def configure(Map<String, Object> config, List<String> stages = ['a', 'b']) {
    def env = config.env
    sh "echo ${env.CHANGE_BRANCH}"
}

def report() {
    [title: 'x'].each { params -> sh "echo ${params.title}" }
}
