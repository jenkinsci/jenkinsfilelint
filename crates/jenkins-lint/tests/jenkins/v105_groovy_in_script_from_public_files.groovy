pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                script {
                    string version = sh(script: 'git describe', returnStdout: true)
                    def flags = [:]
                    def wanted = true
                    wanted ? flags['wanted'] = 'yes' : null
                    def list = [1, 2].collect { it * 2 } join ','
                    def handlers = [[run: { echo 'one' }]
                                    [0]]
                    properties([
                        parameters
                        ([string(name: 'X', defaultValue: 'x')])
                    ])
                    def v = 'a'
                    if (v) {
                        echo 'yes'
                    } else (v == 'b') {
                        echo 'no'
                    }
                    def m = "${'a' + \
                        'b'}"
                    def n = "x${sh(script: '''
                        ls
                    ''', returnStdout: true)}"
                }
            }
        }
    }
}
