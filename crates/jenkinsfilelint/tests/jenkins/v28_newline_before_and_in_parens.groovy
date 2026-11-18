pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def ok = (env.A != 'x'
                        &&
                        (env.B == null || env.B == 'y'))
                }
            }
        }
    }
}
