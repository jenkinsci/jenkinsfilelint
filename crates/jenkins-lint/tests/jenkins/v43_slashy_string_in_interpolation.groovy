pipeline {
    agent any
    stages {
        stage('A') {
            steps {
                script {
                    def m = ['"': 1]
                    echo "${m[/"/]}"
                    echo "${BRANCH_NAME =~ /release\/.+/ ? 'rel' : 'dev'}"
                }
            }
        }
    }
}
