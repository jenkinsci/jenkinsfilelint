pipeline {
    agent any
    stages {
        stage('S') {
            options {
                timeout(time: 5, unit: 'MINUTES')
                retry(2)
                skipDefaultCheckout()
            }
            environment { X = 'y' }
            steps { echo env.X }
            post {
                success { echo 'stage ok' }
            }
        }
    }
}
