pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def v = (env.A
                        ? 'a'
                        : 'b')
                }
            }
        }
    }
}
