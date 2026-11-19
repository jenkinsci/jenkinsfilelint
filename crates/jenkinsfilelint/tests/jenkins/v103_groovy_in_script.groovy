pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                script {
                    java.util.regex.Matcher m = (env.JOB_NAME =~ /(.*)-(.*)/)
                    java.util.LinkedHashMap<java.lang.String, java.lang.Boolean> flags = [a: true]
                    def found = sh(script: 'git describe --always', , returnStdout: true).trim()
                    def items = [1, , 2]
                    def a
                    (a, found) = [1, 2]
                    def pattern = '*.log'
                    sh 'find . -name ' + pattern ' | xargs ls' + ''
                    sh "echo one \
                        two"
                }
            }
        }
    }
}
